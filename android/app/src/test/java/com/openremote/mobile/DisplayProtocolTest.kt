package com.openremote.mobile

import org.junit.Assert.*
import org.junit.Test

class DisplayProtocolTest {
    private val primary = "1".repeat(64)
    private val secondary = "a".repeat(64)
    private fun entry(id: String = primary, name: String = "Display 1", width: String = "1920", height: String = "1080", main: String = "true") =
        """{"id":"$id","name":"$name","width":$width,"height":$height,"primary":$main}"""
    private fun catalog(entries: String = entry(), selected: String = primary) =
        """{"v":1,"type":"display_catalog","selectionMode":"reconnect","selectedMonitorId":"$selected","monitors":[$entries]}"""
    private fun decode(text: String) = DisplayProtocol.decode(text.toByteArray())
    private fun reject(text: String) { assertThrows(Exception::class.java) { decode(text) } }

    @Test fun validCatalogKeepsCurrentAndPrimaryDistinctWithoutDesktopCoordinates() {
        val value = (decode(catalog(entry() + "," + entry(secondary, "Display 2", "1080", "1920", "false"), secondary)) as HostControlMessage.Displays).value
        assertEquals(secondary, value.selected.id)
        assertEquals("Display 2", value.selected.name)
        assertFalse(value.selected.primary)
        assertTrue(value.monitors.first().primary)
        assertEquals(1080, value.selected.width)
        assertEquals(1920, value.selected.height)
    }
    @Test fun legacyCapabilitiesRemainUsableButCannotClaimMonitorSelection() {
        val old = (decode("""{"v":1,"type":"capabilities","text":true,"clipboard":false,"fileTransfer":false,"audio":false,"power":false}""") as HostControlMessage.Capabilities).value
        assertTrue(old.text)
        assertFalse(old.multiMonitor)
        val modern = (decode("""{"v":1,"type":"capabilities","multiMonitor":true}""") as HostControlMessage.Capabilities).value
        assertTrue(modern.multiMonitor)
        listOf("""{"v":1,"type":"capabilities","multiMonitor":1}""", """{"v":1,"type":"capabilities","unknown":false}""",
            """{"v":1,"type":"capabilities","multiMonitor":null}""",
            """{"v":1,"type":"capabilities","text":true,"text":false}""").forEach(::reject)
    }
    @Test fun ambiguousCatalogAndInvalidIdentifiersFailClosed() {
        listOf(catalog(""), catalog(entry() + "," + entry()), catalog(entry(main = "false")),
            catalog(entry() + "," + entry(secondary)), catalog(selected = secondary),
            catalog(entry(id = "A".repeat(64))), catalog(entry(id = "1".repeat(63))),
            catalog(entry(id = "../monitor")), catalog(entry() + "," + entry(secondary, main = "false"), ""),
            catalog((1..17).joinToString(",") { entry(it.toString(16).padStart(64, '0'), main = (it == 1).toString()) }, "1".padStart(64, '0'))).forEach(::reject)
    }
    @Test fun dimensionsLabelsFieldsAndJsonAreStrictlyBounded() {
        listOf(catalog(entry(width = "0")), catalog(entry(width = "32769")), catalog(entry(height = "-1")),
            catalog(entry(width = "1.0")), catalog(entry(width = "\"1920\"")), catalog(entry(main = "\"true\"")),
            catalog(entry(name = "")), catalog(entry(name = "é".repeat(41))), catalog(entry(name = "Display\\u0080")),
            catalog(entry(name = "Display\\u0000")), catalog(entry(name = "Display\\ud800")),
            catalog().replace("\"width\":1920", "\"width\":1920,\"width\":1920"),
            catalog().replace("\"width\":1920", "\"width\":1920,\"path\":\"C:/\""),
            catalog().replace("\"reconnect\"", "\"live\""), catalog().replace("\"v\":1", "\"v\":2"),
            catalog() + "{}", catalog().replace("\"monitors\":[", "\"monitors\":[null,")).forEach(::reject)
        decode(catalog(entry(width = "32768", height = "1", name = "é".repeat(40))))
        assertThrows(Exception::class.java) { DisplayProtocol.decode(ByteArray(16_385) { 32 }) }
        assertThrows(Exception::class.java) { DisplayProtocol.decode(byteArrayOf(0xc3.toByte(), 0x28)) }
        // Existing tools remain array-free even though the display parser opts into bounded arrays.
        assertThrows(Exception::class.java) { ToolProtocol.responseObject("""{"array":[]}""".toByteArray()) }
    }
}
