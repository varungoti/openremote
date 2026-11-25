package com.openremote.mobile

import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class SessionToolsClientTest {
    private fun reply(r: ToolRequest, text: String = "hello") = """{"v":1,"type":"tool_result","requestId":"${r.requestId}","ok":true,"result":{"text":"$text"}}""".toByteArray()
    @Test fun permissionDenialLeavesChannelUsableAndOnlyOneRequestIsOutstanding() = runTest {
        val sent = mutableListOf<ByteArray>(); var closed = false
        val client = SessionToolsClient({ sent += it.bytes; true }, { closed = true }); client.opened()
        val first = ToolProtocol.clipboardRead(); val second = ToolProtocol.clipboardRead()
        val a = async { runCatching { client.request(first) } }
        val b = async { client.request(second) }
        advanceTimeBy(10); runCurrent(); assertEquals(1, sent.size)
        client.receive("""{"v":1,"type":"tool_result","requestId":"${first.requestId}","ok":false,"error":{"code":"permission_denied","message":"Not allowed."}}""".toByteArray())
        runCurrent(); advanceTimeBy(10); runCurrent(); assertEquals(2, sent.size)
        assertTrue(a.await().exceptionOrNull() is ToolOperationException)
        client.receive(reply(second)); assertEquals(ToolResult.ClipboardText("hello"), b.await()); assertFalse(closed)
    }
    @Test fun cancelledRequestLateResponseCannotCompleteTheNextRequest() = runTest {
        var closed = false
        val client = SessionToolsClient({ true }, { closed = true }); client.opened()
        val old = ToolProtocol.clipboardRead(); val next = ToolProtocol.clipboardRead()
        val a = launch { client.request(old) }; advanceTimeBy(10); runCurrent(); a.cancelAndJoin()
        val b = async { client.request(next) }; advanceTimeBy(10); runCurrent()
        client.receive(reply(old, "stale")); assertFalse(b.isCompleted)
        client.receive(reply(next, "current")); assertEquals(ToolResult.ClipboardText("current"), b.await()); assertFalse(closed)
    }
    @Test fun timeoutDoesNotReplayAndClosingFailsPendingRequestsImmediately() = runTest {
        var sent = 0
        val client = SessionToolsClient({ sent++; true }, {}); client.opened()
        val a = async { runCatching { client.request(ToolProtocol.clipboardRead()) } }
        advanceTimeBy(15_011); runCurrent()
        assertTrue(a.await().isFailure); assertEquals(1, sent)
        val b = async { runCatching { client.request(ToolProtocol.clipboardRead()) } }
        advanceTimeBy(10); runCurrent(); client.closed("Session ended"); runCurrent()
        assertTrue(b.isCompleted); assertEquals("Session ended", b.await().exceptionOrNull()?.message)
    }
    @Test fun invalidFrameClosesOnlyTheToolsTransport() = runTest {
        var reason: String? = null
        val client = SessionToolsClient({ true }, { reason = it }); client.opened()
        val a = async { runCatching { client.request(ToolProtocol.clipboardRead()) } }
        advanceTimeBy(10); runCurrent()
        client.receive("""{"v":1,"v":2}""".toByteArray())
        assertTrue(a.await().isFailure); assertNotNull(reason)
    }
}
