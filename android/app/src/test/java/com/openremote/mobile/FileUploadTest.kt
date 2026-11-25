package com.openremote.mobile

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.InputStream
import java.io.File
import java.util.Base64
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class FileUploadTest {
    @get:Rule val temporary = TemporaryFolder()
    private class Receiver : ToolExchange {
        val kinds = mutableListOf<String>()
        val bytes = ArrayList<Byte>()
        var failChunk = false
        var holdChunk = false
        val chunkStarted = CompletableDeferred<Unit>()
        var aborted = false
        override suspend fun request(request: ToolRequest): ToolResult {
            kinds += request.kind
            val json = JSONObject(String(request.bytes))
            return when (request.kind) {
                "file_begin" -> ToolResult.FileAccepted
                "file_chunk" -> {
                    chunkStarted.complete(Unit)
                    if (holdChunk) awaitCancellation()
                    if (failChunk) return ToolResult.ChunkAccepted(-1)
                    val data = Base64.getDecoder().decode(json.getString("data"))
                    assertTrue(data.size <= 8192); assertEquals(bytes.size.toLong(), json.getLong("offset"))
                    bytes.addAll(data.toList()); ToolResult.ChunkAccepted(bytes.size.toLong())
                }
                "file_finish" -> ToolResult.FileFinished(request.expectedName!!, request.expectedSize!!)
                "file_cancel" -> ToolResult.Done
                else -> error("Unexpected test request")
            }
        }
        override fun abort(reason: String) { aborted = true }
    }
    @Test fun uploadWaitsForEveryExactAcknowledgementAndDeletesPrivateCopy() = runBlocking {
        val data = ByteArray(20_001) { (it % 251).toByte() }
        val snapshot = UploadSnapshot.create(temporary.newFolder(), "test.bin", ByteArrayInputStream(data))
        val receiver = Receiver(); val progress = mutableListOf<Long>()
        val result = FileUploader(receiver).upload(snapshot) { sent, total -> assertEquals(data.size.toLong(), total); progress += sent }
        assertEquals(data.size.toLong(), result.size)
        assertEquals(listOf("file_begin", "file_chunk", "file_chunk", "file_chunk", "file_finish"), receiver.kinds)
        assertEquals(listOf(0L, 8192L, 16384L, 20001L), progress)
        assertArrayEquals(data, receiver.bytes.toByteArray()); assertFalse(snapshot.file.exists())
    }
    @Test fun emptyFileHasTheStandardDigestAndNoChunkRequests() = runBlocking {
        val snapshot = UploadSnapshot.create(temporary.newFolder(), "empty.txt", ByteArrayInputStream(byteArrayOf()))
        assertEquals("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", snapshot.sha256)
        val receiver = Receiver(); FileUploader(receiver).upload(snapshot) { _, _ -> }
        assertEquals(listOf("file_begin", "file_finish"), receiver.kinds); assertFalse(snapshot.file.exists())
    }
    @Test fun wrongAcknowledgementCancelsWithoutReplayingOrFinishing() = runBlocking {
        val snapshot = UploadSnapshot.create(temporary.newFolder(), "test.bin", ByteArrayInputStream(ByteArray(20)))
        val receiver = Receiver().apply { failChunk = true }
        assertTrue(runCatching { FileUploader(receiver).upload(snapshot) { _, _ -> } }.isFailure)
        assertEquals(listOf("file_begin", "file_chunk", "file_cancel"), receiver.kinds)
        assertFalse(snapshot.file.exists()); assertFalse(receiver.aborted)
    }
    @Test fun cancellationClosesPendingUploadAndDeletesSnapshot() = runBlocking {
        val snapshot = UploadSnapshot.create(temporary.newFolder(), "test.bin", ByteArrayInputStream(ByteArray(20)))
        val receiver = Receiver().apply { holdChunk = true }
        val job = launch { FileUploader(receiver).upload(snapshot) { _, _ -> } }
        receiver.chunkStarted.await(); job.cancelAndJoin()
        assertEquals(listOf("file_begin", "file_chunk", "file_cancel"), receiver.kinds)
        assertFalse(snapshot.file.exists()); assertFalse(receiver.aborted)
    }
    @Test fun oversizedProviderStreamCannotFillCacheAndPartialCopyIsDeleted() = runBlocking {
        val directory = temporary.newFolder()
        val source = object : InputStream() {
            var remaining = ToolProtocol.MAX_FILE_BYTES + 1
            override fun read(): Int = if (remaining-- > 0) 0 else -1
            override fun read(b: ByteArray, off: Int, len: Int): Int {
                if (remaining == 0L) return -1
                val count = minOf(len.toLong(), remaining).toInt(); remaining -= count; return count
            }
        }
        assertTrue(runCatching { UploadSnapshot.create(directory, "large.bin", source) }.isFailure)
        assertEquals(0, directory.listFiles()!!.size)
    }
    @Test fun evenFailedNativeAbortCannotSkipPrivateFileCleanup() = runBlocking {
        val snapshot = UploadSnapshot.create(temporary.newFolder(), "test.bin", ByteArrayInputStream(byteArrayOf(1)))
        val broken = object : ToolExchange {
            override suspend fun request(request: ToolRequest): ToolResult = if (request.kind == "file_begin") ToolResult.FileAccepted else throw IllegalStateException("channel disposed")
            override fun abort(reason: String) { throw IllegalStateException("native close failed") }
        }
        assertTrue(runCatching { FileUploader(broken).upload(snapshot) { _, _ -> } }.isFailure)
        assertFalse(snapshot.file.exists())
    }
    @Test fun interruptedSnapshotDeletesItsPartialCopy() = runBlocking {
        val directory = temporary.newFolder()
        val started = CountDownLatch(1); val resume = CountDownLatch(1)
        val source = object : InputStream() {
            override fun read(): Int = 1
            override fun read(b: ByteArray, off: Int, len: Int): Int {
                started.countDown(); check(resume.await(5, TimeUnit.SECONDS)); b[off] = 1; return 1
            }
        }
        val job = launch { UploadSnapshot.create(directory, "partial.bin", source) }
        assertTrue(withContext(Dispatchers.IO) { started.await(5, TimeUnit.SECONDS) })
        job.cancel(); resume.countDown(); job.join()
        assertEquals(0, directory.listFiles()!!.size)
    }
    @Test fun cancelledReservationsStillConsumeSessionQuota() = runBlocking {
        val receiver = Receiver(); val uploader = FileUploader(receiver)
        repeat(5) {
            // A truncated private copy forces failure after reservation, without writing 320 MiB in this test.
            val snapshot = UploadSnapshot(temporary.newFile(), "truncated.bin", 67_108_864, "0".repeat(64))
            assertTrue(runCatching { uploader.upload(snapshot) { _, _ -> } }.isFailure)
            assertFalse(snapshot.file.exists())
        }
        assertEquals(4, receiver.kinds.count { it == "file_begin" })
        assertEquals(4, receiver.kinds.count { it == "file_cancel" })
        assertFalse(receiver.kinds.contains("file_finish"))
    }
    @Test fun rejectedBeginKeepsToolsUsableAndDeletesTheUnsentSnapshot() = runBlocking {
        val snapshot = UploadSnapshot.create(temporary.newFolder(), "test.bin", ByteArrayInputStream(byteArrayOf(1)))
        val kinds = mutableListOf<String>(); var aborted = false
        val denied = object : ToolExchange {
            override suspend fun request(request: ToolRequest): ToolResult { kinds += request.kind; throw ToolOperationException("permission_denied") }
            override fun abort(reason: String) { aborted = true }
        }
        val result = runCatching { FileUploader(denied).upload(snapshot) { _, _ -> } }
        assertTrue(result.exceptionOrNull() is ToolOperationException)
        assertEquals(listOf("file_begin"), kinds); assertFalse(aborted); assertFalse(snapshot.file.exists())
    }
    @Test fun anotherActivityCannotDeleteAnActivePrivateSnapshot() {
        val directory = temporary.newFolder()
        val stale = File(directory, "upload-stale.pending").apply { writeText("stale") }
        val unrelated = File(directory, "another-feature.bin").apply { writeText("untouched") }
        val recovery = UploadCacheRecovery()
        recovery.prepare(directory); assertFalse(stale.exists()); assertTrue(unrelated.exists())
        val active = File(directory, "upload-active.pending").apply { writeText("in progress") }
        recovery.prepare(directory)
        assertTrue(active.exists()); assertEquals("in progress", active.readText()); assertTrue(unrelated.exists())
    }
}
