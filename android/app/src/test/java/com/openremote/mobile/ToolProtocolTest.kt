package com.openremote.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class ToolProtocolTest {
    private val transfer = "0123456789abcdef0123456789abcdef"
    private val emptyHash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    private fun reply(request: ToolRequest, result: String) = """{"v":1,"type":"tool_result","requestId":"${request.requestId}","ok":true,"result":$result}""".toByteArray()

    @Test fun textLimitsMeasureUtf8AndRejectNulAndBrokenUnicode() {
        assertTrue(ToolProtocol.validText(""))
        assertFalse(ToolProtocol.validText("", allowEmpty = false))
        assertTrue(ToolProtocol.validText("😀".repeat(1024)))
        assertFalse(ToolProtocol.validText("😀".repeat(1025)))
        assertFalse(ToolProtocol.validText("hello\u0000world"))
        assertFalse(ToolProtocol.validText("\ud800"))
        assertFalse(ToolProtocol.validTypedText("hello\u0000world"))
        assertFalse(ToolProtocol.validTypedText("\u0001".repeat(4096)))
        assertTrue(ToolProtocol.validTypedText("Hello 😀"))
        assertThrows(IllegalArgumentException::class.java) { ToolProtocol.clipboardWrite("\u0000") }
        // Valid raw text can still exceed the JSON frame budget after escaping.
        assertThrows(IllegalArgumentException::class.java) { ToolProtocol.clipboardWrite("\u0001".repeat(4096)) }
    }
    @Test fun rejectsTraversalDeviceNamesAndOversizedFilesBeforeSending() {
        listOf("../secret", "a\\b", "CON", "Nul.txt", "CONIN$", "conout$.txt", "CON .log", "CON\u00a0.log", "CON\u0131N$",
            "bad\u0085name.txt", "COM1.doc", "LPT9.txt", "COM¹.txt", "a:b", "name.", "name ", "", "a".repeat(121)).forEach {
            assertFalse(it, ToolProtocol.validName(it))
        }
        assertTrue(ToolProtocol.validName("Quarterly notes 📝.txt"))
        assertThrows(IllegalArgumentException::class.java) { ToolProtocol.fileBegin(transfer, "a.txt", 67_108_865L, emptyHash) }
        assertThrows(IllegalArgumentException::class.java) { ToolProtocol.fileChunk(transfer, 0, ByteArray(8193)) }
        assertThrows(IllegalArgumentException::class.java) { ToolProtocol.fileChunk(transfer, -1, byteArrayOf(1)) }
    }
    @Test fun strictJsonRejectsDuplicatesCoercionUnknownFieldsAndMalformedUnicode() {
        val r = ToolProtocol.clipboardRead()
        val good = String(reply(r, """{"text":"hello"}"""))
        val malformed = listOf(
            good.replace("\"v\":1", "\"v\":1,\"v\":1"),
            good.replace("\"v\":1", "\"v\":1,\"\\u0076\":1"),
            good.replace("\"text\":\"hello\"", "\"text\":\"hello\",\"text\":\"secret\""),
            good.replace("\"v\":1", "\"v\":\"1\""), good.replace("\"v\":1", "\"v\":1.0"),
            good.replace("\"v\":1", "\"v\":01"), good.replace("\"v\":1", "\"v\":2"),
            good.replace("\"ok\":true", "\"ok\":\"true\""), good.replace("\"text\":\"hello\"", "\"text\":null"),
            good.replace("\"text\":\"hello\"", "\"text\":\"\\ud800\""),
            good.replace("\"text\":\"hello\"", "\"text\":\"hello\",\"path\":\"/secret\""), good + "{}",
        )
        malformed.forEach { bytes -> assertThrows(Exception::class.java) { ToolProtocol.decodeResponse(r, bytes.toByteArray()) } }
        assertThrows(Exception::class.java) { ToolProtocol.decodeResponse(r, byteArrayOf(0xc3.toByte(), 0x28)) }
        assertThrows(Exception::class.java) { ToolProtocol.decodeResponse(r, ByteArray(16_385)) }
        assertEquals(ToolResult.ClipboardText("hello"), ToolProtocol.decodeResponse(r, good.toByteArray()))
    }
    @Test fun acknowledgementsMustMatchRequestTransferAndExactNextOffset() {
        val r = ToolProtocol.fileChunk(transfer, 8192, byteArrayOf(1, 2, 3))
        assertEquals(ToolResult.ChunkAccepted(8195), ToolProtocol.decodeResponse(r, reply(r, """{"transferId":"$transfer","received":8195}""")))
        listOf("8192", "8196", "\"8195\"", "8195.0", "-1").forEach {
            assertThrows(Exception::class.java) { ToolProtocol.decodeResponse(r, reply(r, """{"transferId":"$transfer","received":$it}""")) }
        }
        assertThrows(Exception::class.java) { ToolProtocol.decodeResponse(r, reply(r, """{"transferId":"ffffffffffffffffffffffffffffffff","received":8195}""")) }
        val other = ToolProtocol.fileChunk(transfer, 8192, byteArrayOf(1, 2, 3))
        assertThrows(Exception::class.java) { ToolProtocol.decodeResponse(r, reply(other, """{"transferId":"$transfer","received":8195}""")) }
    }
    @Test fun emptyFileCompletionRequiresExpectedSavedNameSizeAndDigest() {
        val r = ToolProtocol.fileFinish(transfer, "empty.txt", 0, emptyHash)
        val result = """{"transferId":"$transfer","name":"${transfer}_empty.txt","size":0,"sha256":"$emptyHash"}"""
        assertEquals(ToolResult.FileFinished("${transfer}_empty.txt", 0), ToolProtocol.decodeResponse(r, reply(r, result)))
        listOf(result.replace("\"size\":0", "\"size\":1"), result.replace(emptyHash, "0".repeat(64)), result.replace("${transfer}_empty.txt", "another.txt")).forEach {
            assertThrows(Exception::class.java) { ToolProtocol.decodeResponse(r, reply(r, it)) }
        }
    }
    @Test fun permissionDenialIsAnOperationErrorAndNeverContainsRemoteContent() {
        val r = ToolProtocol.clipboardRead()
        val bytes = """{"v":1,"type":"tool_result","requestId":"${r.requestId}","ok":false,"error":{"code":"permission_denied","message":"private path or clipboard text"}}""".toByteArray()
        val error = assertThrows(ToolOperationException::class.java) { ToolProtocol.decodeResponse(r, bytes) }
        assertEquals("permission_denied", error.code)
        assertFalse(error.message!!.contains("private path"))
        val begin = JSONObject(String(ToolProtocol.fileBegin(transfer, "empty.txt", 0, emptyHash).bytes))
        assertEquals(0, begin.getLong("size"))
        assertEquals(emptyHash, begin.getString("sha256"))
    }
}
