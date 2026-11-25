package com.openremote.mobile

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import java.io.Closeable
import java.io.File
import java.io.InputStream
import java.security.MessageDigest

/** One recovery per app process; a second Activity must not delete a live Activity's snapshot. */
internal class UploadCacheRecovery {
    private val prepared = hashSetOf<String>()
    @Synchronized fun prepare(directory: File): File {
        if (prepared.add(directory.absolutePath)) {
            directory.listFiles()?.filter { it.isFile && it.name.startsWith("upload-") && it.name.endsWith(".pending") }?.forEach { it.delete() }
        }
        return directory
    }
}

/** Immutable private copy: a document provider cannot change bytes between hashing and upload. */
class UploadSnapshot internal constructor(val file: File, val name: String, val size: Long, val sha256: String) : Closeable {
    override fun close() { if (file.exists() && !file.delete()) file.deleteOnExit() }
    companion object {
        suspend fun create(directory: File, name: String, input: InputStream): UploadSnapshot {
            var created: File? = null
            var delivered = false
            try {
                return withContext(Dispatchers.IO) {
                    require(ToolProtocol.validName(name)) { "This filename is not supported on Windows. Rename it before sending." }
                    require(directory.isDirectory || directory.mkdirs()) { "The private upload cache is unavailable" }
                    val file = File.createTempFile("upload-", ".pending", directory)
                    created = file
                    var keep = false
                    try {
                        val digest = MessageDigest.getInstance("SHA-256")
                        var size = 0L
                        file.outputStream().use { out ->
                            val buffer = ByteArray(ToolProtocol.CHUNK_BYTES)
                            while (true) {
                                currentCoroutineContext().ensureActive()
                                val count = input.read(buffer)
                                if (count < 0) break
                                if (count == 0) continue
                                require(size + count <= ToolProtocol.MAX_FILE_BYTES) { "Files must be at most 64 MiB" }
                                out.write(buffer, 0, count); digest.update(buffer, 0, count); size += count
                            }
                            out.flush()
                        }
                        currentCoroutineContext().ensureActive()
                        val result = UploadSnapshot(file, name, size, digest.digest().joinToString("") { "%02x".format(it) })
                        keep = true
                        result
                    } finally { if (!keep) file.delete() }
                }.also { delivered = true }
            } finally { if (!delivered) created?.delete() }
        }
    }
}

class FileUploader(private val exchange: ToolExchange) {
    private var reserved = 0L
    private var active = false

    /** Takes ownership of the snapshot and deletes it on success, denial, timeout or cancellation. */
    suspend fun upload(snapshot: UploadSnapshot, progress: (Long, Long) -> Unit): ToolResult.FileFinished {
        var began = false
        var completed = false
        var ownsSlot = false
        val id = ToolProtocol.newId()
        try {
            check(!active) { "Another file upload is active" }
            active = true; ownsSlot = true
            require(snapshot.size in 0..ToolProtocol.MAX_FILE_BYTES && reserved + snapshot.size <= ToolProtocol.SESSION_FILE_BYTES) { "This session has reached its 256 MiB upload allowance. Reconnect before sending more." }
            reserved += snapshot.size // Reservations remain spent after cancellation or failure, matching the host.
            currentCoroutineContext().ensureActive()
            began = true
            val acceptance = try { exchange.request(ToolProtocol.fileBegin(id, snapshot.name, snapshot.size, snapshot.sha256)) }
            catch (e: ToolOperationException) { began = false; throw e } // A definitive begin rejection created no active upload.
            check(acceptance == ToolResult.FileAccepted) { "The PC did not accept the file" }
            var offset = 0L
            progress(offset, snapshot.size)
            snapshot.file.inputStream().use { input ->
                while (offset < snapshot.size) {
                    currentCoroutineContext().ensureActive()
                    val wanted = minOf(ToolProtocol.CHUNK_BYTES.toLong(), snapshot.size - offset).toInt()
                    val chunk = withContext(Dispatchers.IO) {
                        val data = ByteArray(wanted)
                        var count = 0
                        while (count < wanted) {
                            val n = input.read(data, count, wanted - count)
                            check(n > 0) { "The private upload copy is incomplete" }; count += n
                        }
                        data
                    }
                    val result = exchange.request(ToolProtocol.fileChunk(id, offset, chunk))
                    check(result is ToolResult.ChunkAccepted && result.received == offset + chunk.size) { "The PC did not acknowledge this chunk" }
                    offset += chunk.size
                    progress(offset, snapshot.size)
                }
                check(withContext(Dispatchers.IO) { input.read() } == -1) { "The private upload copy changed" }
            }
            val result = exchange.request(ToolProtocol.fileFinish(id, snapshot.name, snapshot.size, snapshot.sha256))
            check(result is ToolResult.FileFinished && result.name == "${id}_${snapshot.name}" && result.size == snapshot.size) { "File completion was not confirmed" }
            completed = true
            return result
        } finally {
            try {
                if (began && !completed) {
                    withContext(NonCancellable) {
                        // Never replay a chunk. An uncertain cancellation closes only tools so the host can clean up.
                        try { withTimeout(3_000) { exchange.request(ToolProtocol.fileCancel(id)) } }
                        catch (_: Exception) { runCatching { exchange.abort("Upload cancellation could not be confirmed. Session tools were closed; the desktop remains available.") } }
                    }
                }
            } finally {
                try { snapshot.close() } finally { if (ownsSlot) active = false }
            }
        }
    }
}
