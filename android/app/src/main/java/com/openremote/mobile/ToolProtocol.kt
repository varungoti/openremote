package com.openremote.mobile

import org.json.JSONObject
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.util.Base64
import java.util.UUID
import java.util.Locale

/** The bounded v1 wire contract in docs/session-tools.md. No clipboard/file content is logged. */
object ToolProtocol {
    const val MAX_FRAME_BYTES = 16_384
    const val MAX_TEXT_BYTES = 4_096
    const val CHUNK_BYTES = 8_192
    const val MAX_FILE_BYTES = 67_108_864L
    const val SESSION_FILE_BYTES = 268_435_456L
    private val idPattern = Regex("[0-9a-f]{32}")
    private val hashPattern = Regex("[0-9a-f]{64}")
    private val reservedNames = setOf("CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$")
    private val numberedDevice = Regex("(COM|LPT)[1-9¹²³]")

    fun newId(): String = UUID.randomUUID().toString().replace("-", "")
    fun validText(value: String, allowEmpty: Boolean = true): Boolean =
        (allowEmpty || value.isNotEmpty()) && '\u0000' !in value && wellFormedUnicode(value) &&
            value.toByteArray(Charsets.UTF_8).size <= MAX_TEXT_BYTES
    fun validTypedText(value: String): Boolean = validText(value, allowEmpty = false) &&
        JSONObject().put("v", 1).put("type", "text").put("text", value).toString().toByteArray(Charsets.UTF_8).size <= MAX_FRAME_BYTES

    fun validName(value: String): Boolean = value.isNotEmpty() && wellFormedUnicode(value) &&
        value.toByteArray(Charsets.UTF_8).size <= 120 && value.none { Character.isISOControl(it) || it in "<>:\"/\\|?*" } &&
        !value.endsWith('.') && !value.endsWith(' ') && !reservedStem(value)
    private fun reservedStem(value: String): Boolean {
        val stem = value.substringBefore('.').trimEnd().uppercase(Locale.ROOT)
        return stem in reservedNames || numberedDevice.matches(stem)
    }

    fun wellFormedUnicode(value: String): Boolean {
        var i = 0
        while (i < value.length) {
            val c = value[i++]
            if (c.isHighSurrogate()) {
                if (i >= value.length || !value[i++].isLowSurrogate()) return false
            } else if (c.isLowSurrogate()) return false
        }
        return true
    }

    fun clipboardRead() = request("clipboard_read")
    fun clipboardWrite(text: String): ToolRequest {
        require(validText(text)) { "Clipboard text must be valid Unicode without NUL and at most 4,096 UTF-8 bytes" }
        return request("clipboard_write", JSONObject().put("text", text))
    }
    fun fileBegin(transferId: String, name: String, size: Long, sha256: String): ToolRequest {
        require(validName(name)) { "Choose a filename of at most 120 UTF-8 bytes that is valid on Windows" }
        require(size in 0..MAX_FILE_BYTES) { "Files must be at most 64 MiB" }
        require(hashPattern.matches(sha256)) { "Invalid file digest" }
        return request("file_begin", JSONObject().put("transferId", checkedId(transferId)).put("name", name).put("size", size).put("sha256", sha256),
            transferId = transferId)
    }
    fun fileChunk(transferId: String, offset: Long, data: ByteArray): ToolRequest {
        require(offset in 0..MAX_FILE_BYTES && data.size in 1..CHUNK_BYTES && offset + data.size <= MAX_FILE_BYTES) { "Invalid file chunk" }
        return request("file_chunk", JSONObject().put("transferId", checkedId(transferId)).put("offset", offset)
            .put("data", Base64.getEncoder().encodeToString(data)), transferId = transferId, expectedReceived = offset + data.size)
    }
    fun fileFinish(transferId: String, name: String, size: Long, sha256: String): ToolRequest {
        require(validName(name) && size in 0..MAX_FILE_BYTES && hashPattern.matches(sha256)) { "Invalid file confirmation" }
        return request("file_finish", JSONObject().put("transferId", checkedId(transferId)), transferId = transferId,
            expectedName = "${transferId}_$name", expectedSize = size, expectedHash = sha256)
    }
    fun fileCancel(transferId: String) = request("file_cancel", JSONObject().put("transferId", checkedId(transferId)), transferId = transferId)
    private fun checkedId(id: String): String { require(idPattern.matches(id)) { "Invalid tools identifier" }; return id }
    private fun request(kind: String, fields: JSONObject = JSONObject(), transferId: String? = null, expectedReceived: Long? = null,
                        expectedName: String? = null, expectedSize: Long? = null, expectedHash: String? = null): ToolRequest {
        val id = newId()
        val data = fields.put("v", 1).put("type", kind).put("requestId", id).toString().toByteArray(Charsets.UTF_8)
        require(data.size <= MAX_FRAME_BYTES) { "The encoded request is too large" }
        return ToolRequest(kind, id, data, transferId, expectedReceived, expectedName, expectedSize, expectedHash)
    }

    internal fun responseObject(bytes: ByteArray): Map<String, Any> = StrictToolJson.parse(bytes)
    internal fun responseId(value: Map<String, Any>): String = value.string("requestId").also { checkedId(it) }
    fun decodeResponse(request: ToolRequest, bytes: ByteArray): ToolResult = decodeResponse(request, responseObject(bytes))
    internal fun decodeResponse(request: ToolRequest, value: Map<String, Any>): ToolResult {
        require(value["v"] == 1L && value["type"] == "tool_result" && responseId(value) == request.requestId) { "Unexpected tools response" }
        val ok = value["ok"] as? Boolean ?: throw IllegalArgumentException("Invalid tools result flag")
        value.exactKeys(if (ok) setOf("v", "type", "requestId", "ok", "result") else setOf("v", "type", "requestId", "ok", "error"))
        if (!ok) {
            val error = value.obj("error")
            error.exactKeys(setOf("code", "message"))
            val code = error.string("code")
            val message = error.string("message")
            require(Regex("[a-z][a-z0-9_]{0,63}").matches(code) && message.toByteArray(Charsets.UTF_8).size <= 1024 && '\u0000' !in message) { "Invalid tools error" }
            // The UI uses our bounded messages, never arbitrary remote paths/content.
            throw ToolOperationException(code)
        }
        val result = value.obj("result")
        return when (request.kind) {
            "clipboard_read" -> {
                result.exactKeys(setOf("text"))
                val text = result.string("text")
                require(validText(text)) { "Invalid clipboard result" }
                ToolResult.ClipboardText(text)
            }
            "clipboard_write" -> { result.exactKeys(emptySet()); ToolResult.Done }
            "file_begin" -> {
                result.exactKeys(setOf("transferId", "received", "chunkBytes"))
                require(result.string("transferId") == request.transferId && result["received"] == 0L && result["chunkBytes"] == CHUNK_BYTES.toLong()) { "Invalid upload acceptance" }
                ToolResult.FileAccepted
            }
            "file_chunk" -> {
                result.exactKeys(setOf("transferId", "received"))
                require(result.string("transferId") == request.transferId && result["received"] == request.expectedReceived) { "File acknowledgement does not match the chunk" }
                ToolResult.ChunkAccepted(request.expectedReceived!!)
            }
            "file_finish" -> {
                result.exactKeys(setOf("transferId", "name", "size", "sha256"))
                require(result.string("transferId") == request.transferId && result.string("name") == request.expectedName &&
                    result["size"] == request.expectedSize && result.string("sha256") == request.expectedHash) { "File completion did not confirm its length and digest" }
                ToolResult.FileFinished(result.string("name"), request.expectedSize!!)
            }
            "file_cancel" -> {
                result.exactKeys(setOf("transferId"))
                require(result.string("transferId") == request.transferId) { "Invalid cancellation acknowledgement" }
                ToolResult.Done
            }
            else -> throw IllegalArgumentException("Unsupported tools response")
        }
    }
    private fun Map<String, Any>.exactKeys(expected: Set<String>) { require(keys == expected) { "Unexpected tools fields" } }
    private fun Map<String, Any>.string(key: String): String = this[key] as? String ?: throw IllegalArgumentException("Invalid tools string")
    @Suppress("UNCHECKED_CAST")
    private fun Map<String, Any>.obj(key: String): Map<String, Any> = this[key] as? Map<String, Any> ?: throw IllegalArgumentException("Invalid tools object")
}

class ToolRequest internal constructor(
    val kind: String, val requestId: String, val bytes: ByteArray, val transferId: String? = null,
    val expectedReceived: Long? = null, val expectedName: String? = null, val expectedSize: Long? = null, val expectedHash: String? = null,
)
sealed interface ToolResult {
    data class ClipboardText(val text: String) : ToolResult
    data object Done : ToolResult
    data object FileAccepted : ToolResult
    data class ChunkAccepted(val received: Long) : ToolResult
    data class FileFinished(val name: String, val size: Long) : ToolResult
}
class ToolOperationException(val code: String) : Exception(when (code) {
    "permission_denied" -> "The PC did not permit this action. Reconnect with the required permission."
    "clipboard_empty", "clipboard_not_text", "clipboard_no_text", "no_text" -> "The PC clipboard does not contain readable text."
    "clipboard_too_large", "text_too_large", "clipboard_text_limit" -> "The PC clipboard exceeds the 4,096-byte text limit or contains unsupported NUL text."
    "response_too_large" -> "The PC clipboard cannot fit in one text response. Copy a shorter passage on the PC."
    "clipboard_unavailable", "clipboard_busy" -> "The PC clipboard is temporarily unavailable. Try again."
    "quota_exceeded" -> "This session has used its 256 MiB upload allowance. Reconnect before sending more."
    "transfer_busy" -> "Another upload is still active on the PC."
    "digest_mismatch", "hash_mismatch", "length_mismatch" -> "The PC could not verify the file length or digest. The upload was not accepted."
    "file_io" -> "The PC could not store this file. Check its free disk space and the ReceivedFiles folder."
    "zone_metadata", "unsupported_filesystem" -> "The PC could not apply Internet-zone metadata. Use a supported local Windows filesystem for received files."
    else -> "The PC could not complete this action ($code)."
})

/** Android's JSONObject permits duplicate keys/coercion, so incoming security boundaries use this small strict parser. */
internal object StrictToolJson {
    fun parse(bytes: ByteArray, allowArrays: Boolean = false): Map<String, Any> {
        require(bytes.size in 1..ToolProtocol.MAX_FRAME_BYTES) { "Invalid tools frame size" }
        val text = Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT)
            .decode(ByteBuffer.wrap(bytes)).toString()
        return Parser(text, allowArrays).parse()
    }
    private class Parser(private val source: String, private val allowArrays: Boolean) {
        private var at = 0
        fun parse(): Map<String, Any> { val result = obj(0); whitespace(); require(at == source.length) { "Trailing tools data" }; return result }
        private fun whitespace() { while (at < source.length && source[at] in " \n\r\t") at++ }
        private fun take(c: Char) { whitespace(); require(at < source.length && source[at++] == c) { "Malformed tools JSON" } }
        private fun obj(depth: Int): Map<String, Any> {
            require(depth <= 3) { "Tools JSON is too deeply nested" }; take('{')
            val result = linkedMapOf<String, Any>(); whitespace()
            if (at < source.length && source[at] == '}') { at++; return result }
            while (true) {
                val key = string(); require(key.length <= 64 && !result.containsKey(key) && result.size < 16) { "Duplicate or invalid tools fields" }
                take(':'); whitespace(); require(at < source.length) { "Incomplete tools JSON" }
                result[key] = when (source[at]) {
                    '{' -> obj(depth + 1)
                    '[' -> array(depth + 1)
                    '"' -> string()
                    't' -> { literal("true"); true }
                    'f' -> { literal("false"); false }
                    '-', in '0'..'9' -> number()
                    else -> throw IllegalArgumentException("Unsupported tools JSON value")
                }
                whitespace(); require(at < source.length) { "Incomplete tools JSON" }
                when (source[at++]) { '}' -> return result; ',' -> Unit; else -> throw IllegalArgumentException("Malformed tools JSON") }
            }
        }
        private fun array(depth: Int): List<Any> {
            require(allowArrays && depth <= 3) { "Unexpected tools array" }; take('[')
            val result = mutableListOf<Any>(); whitespace()
            if (at < source.length && source[at] == ']') { at++; return result }
            while (true) {
                require(result.size < 16) { "Too many array entries" }
                // Only the display catalog uses arrays, and its entries must be objects.
                result.add(obj(depth + 1)); whitespace()
                require(at < source.length) { "Incomplete tools JSON" }
                when (source[at++]) { ']' -> return result; ',' -> Unit; else -> throw IllegalArgumentException("Malformed tools JSON") }
            }
        }
        private fun literal(value: String) { require(source.startsWith(value, at)) { "Invalid tools literal" }; at += value.length }
        private fun number(): Long {
            val start = at
            if (source[at] == '-') at++
            require(at < source.length && source[at] in '0'..'9') { "Invalid tools integer" }
            if (source[at] == '0') at++ else while (at < source.length && source[at] in '0'..'9') at++
            return source.substring(start, at).toLongOrNull() ?: throw IllegalArgumentException("Tools integer is out of range")
        }
        private fun string(): String {
            take('"'); val result = StringBuilder()
            while (at < source.length) {
                val c = source[at++]
                if (c == '"') return result.toString().also { require(ToolProtocol.wellFormedUnicode(it)) { "Invalid tools Unicode" } }
                require(c.code >= 32) { "Unescaped tools control character" }
                if (c != '\\') result.append(c) else {
                    require(at < source.length) { "Invalid tools escape" }
                    result.append(when (val escape = source[at++]) {
                        '"', '\\', '/' -> escape
                        'b' -> '\b'; 'f' -> '\u000c'; 'n' -> '\n'; 'r' -> '\r'; 't' -> '\t'
                        'u' -> { require(at + 4 <= source.length) { "Invalid tools Unicode escape" }
                            val hex = source.substring(at, at + 4); require(hex.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) { "Invalid tools Unicode escape" }
                            at += 4; hex.toInt(16).toChar() }
                        else -> throw IllegalArgumentException("Invalid tools escape")
                    })
                }
            }
            throw IllegalArgumentException("Unterminated tools string")
        }
    }
}
