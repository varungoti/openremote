package com.openremote.mobile

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout

interface ToolExchange {
    suspend fun request(request: ToolRequest): ToolResult
    fun abort(reason: String)
}

/** One in-flight request. Cancellation retires its ID; a late reply cannot satisfy a newer request. */
class SessionToolsClient(
    private val send: (ToolRequest) -> Boolean,
    private val closeChannel: (String) -> Unit,
) : ToolExchange {
    private data class Pending(val request: ToolRequest, val result: CompletableDeferred<ToolResult>)
    private val requests = Mutex()
    private val lock = Any()
    private var pending: Pending? = null
    private val retired = linkedMapOf<String, ToolRequest>()
    private var open = false
    private var ended = false

    fun opened() = synchronized(lock) { if (!ended) open = true }
    fun closed(reason: String) = synchronized(lock) {
        open = false; ended = true
        pending?.result?.completeExceptionally(IllegalStateException(reason))
        pending = null
        retired.clear()
    }

    override suspend fun request(request: ToolRequest): ToolResult = requests.withLock {
        // Includes begin/finish/cancel. Even a zero-latency peer stays below 128 requests/second.
        delay(10)
        val current = Pending(request, CompletableDeferred())
        synchronized(lock) {
            check(open && !ended) { "Session tools are unavailable. Reconnect to the PC." }
            check(pending == null) { "Another tools request is pending" }
            pending = current
        }
        try {
            check(send(request)) { "The tools request could not be sent" }
            withTimeout(15_000) { current.result.await() }
        } finally {
            synchronized(lock) {
                if (pending === current) {
                    pending = null
                    retired[request.requestId] = request
                    while (retired.size > 32) retired.remove(retired.keys.first())
                }
            }
        }
    }

    fun receive(bytes: ByteArray) {
        try {
            val value = ToolProtocol.responseObject(bytes)
            val id = ToolProtocol.responseId(value)
            val entry: Pending?
            val request: ToolRequest
            synchronized(lock) {
                if (ended) return
                entry = pending?.takeIf { it.request.requestId == id }
                request = entry?.request ?: retired[id] ?: throw IllegalArgumentException("Unexpected tools request identifier")
            }
            try {
                val result = ToolProtocol.decodeResponse(request, value)
                if (entry != null) entry.result.complete(result)
            } catch (e: ToolOperationException) {
                // Permission denial or I/O failure is an operation result, never a desktop failure.
                entry?.result?.completeExceptionally(e)
            }
            synchronized(lock) {
                if (pending === entry && entry != null) pending = null
                retired.remove(id)
            }
        } catch (_: Exception) {
            abort("The PC returned an invalid tools response. The desktop is still connected.")
        }
    }

    override fun abort(reason: String) {
        val notify = synchronized(lock) { !ended }
        closed(reason)
        if (notify) runCatching { closeChannel(reason) }
    }
}
