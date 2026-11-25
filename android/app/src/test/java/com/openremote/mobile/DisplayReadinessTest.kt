package com.openremote.mobile

import org.junit.Assert.*
import org.junit.Test

class DisplayReadinessTest {
    private val primary = "1".repeat(64)
    private val secondary = "2".repeat(64)
    private val monitors = listOf(RemoteMonitor(primary, "Display 1", 1920, 1080, true), RemoteMonitor(secondary, "Display 2", 1080, 1920, false))
    private val catalog = DisplayCatalog(secondary, monitors)

    @Test fun explicitMonitorCannotPublishVideoOrEnableInputBeforeExactBinding() {
        val state = DisplayReadiness(secondary)
        val renderer = state.attachRenderer()
        state.rendered(renderer)
        assertFalse(state.bindingReady)
        assertFalse(state.ready(true))
        state.capabilities(HostCapabilities(multiMonitor = true))
        assertFalse(state.bindingReady) // Even decoded pixels remain unpublished before the catalog.
        assertThrows(IllegalArgumentException::class.java) { state.catalog(DisplayCatalog(primary, monitors)) }
        assertFalse(state.bindingReady)
        state.catalog(catalog)
        assertTrue(state.bindingReady)
        assertTrue(state.ready(true))
        assertFalse(state.ready(false))
    }
    @Test fun explicitSelectionRejectsLegacyHostsAndChangedTopologyWhilePrimarySupportsLegacy() {
        val explicit = DisplayReadiness(secondary)
        assertThrows(IllegalArgumentException::class.java) { explicit.capabilities(HostCapabilities(text = true)) }
        assertFalse(explicit.bindingReady)
        val implicit = DisplayReadiness(null)
        implicit.capabilities(HostCapabilities(text = true))
        assertTrue(implicit.bindingReady)
        assertNull(implicit.catalog)
        val current = DisplayReadiness(null)
        assertThrows(IllegalArgumentException::class.java) { current.catalog(catalog) }
        current.capabilities(HostCapabilities(multiMonitor = true))
        assertFalse(current.bindingReady)
        current.catalog(catalog)
        assertThrows(IllegalArgumentException::class.java) { current.catalog(DisplayCatalog(primary, monitors)) }
        assertThrows(IllegalArgumentException::class.java) { current.capabilities(HostCapabilities()) }
    }
    @Test fun rendererRecreationRejectsOldFramesAndOldDisposalWithoutResettingBinding() {
        val state = DisplayReadiness(secondary)
        state.capabilities(HostCapabilities(multiMonitor = true)); state.catalog(catalog)
        val old = state.attachRenderer(); state.rendered(old)
        assertTrue(state.ready(true))
        val new = state.attachRenderer()
        assertTrue(state.bindingReady)
        assertFalse(state.ready(true))
        state.rendered(old)
        assertFalse(state.ready(true))
        state.rendered(new)
        state.detachRenderer(old)
        assertTrue(state.ready(true))
        state.detachRenderer(new)
        assertFalse(state.ready(true))
        state.rendered(new)
        assertFalse(state.ready(true))
    }
    @Test fun longLivedSessionGetsNewRendererDeadlineAndObsoleteCallbacksCannotChangeIt() {
        val state = DisplayReadiness(null)
        state.capabilities(HostCapabilities())
        val old = state.attachRenderer(0); state.rendered(old)
        assertFalse(state.rendererTimedOut(120_000))
        assertTrue(state.detachRenderer(old, 120_000))
        assertFalse(state.rendererTimedOut(120_001)) // Stats tick between old disposal and new attach.
        assertFalse(state.rendererTimedOut(149_999))
        assertTrue(state.rendererTimedOut(150_000)) // No replacement renderer cannot wait indefinitely.
        val current = state.attachRenderer(150_010)
        assertFalse(state.detachRenderer(old, 179_999))
        state.rendered(old)
        assertFalse(state.rendererTimedOut(180_009))
        assertTrue(state.rendererTimedOut(180_010)) // Old detach cannot extend the current deadline either.
        state.rendered(current)
        assertFalse(state.rendererTimedOut(900_000))
    }
    @Test fun explicitPickerPausesOnlyRenderDeadlineAndRequiresFreshRenderAfterRecreation() {
        val state = DisplayReadiness(secondary)
        state.capabilities(HostCapabilities(multiMonitor = true)); state.catalog(catalog)
        val old = state.attachRenderer(0); state.rendered(old)
        state.pickerHandoff(true, 60_000)
        assertFalse(state.ready(true)) // Input/new tools are paused even before renderer recreation.
        state.detachRenderer(old, 61_000)
        val current = state.attachRenderer(62_000)
        assertFalse(state.rendererTimedOut(179_999)) // ViewModel owns the independent 120-second picker limit.
        state.pickerHandoff(false, 179_999)
        assertFalse(state.ready(true))
        assertFalse(state.rendererTimedOut(209_998))
        assertTrue(state.rendererTimedOut(209_999))
        state.rendered(old)
        assertFalse(state.ready(true))
        state.rendered(current)
        assertTrue(state.ready(true))
    }
    @Test fun slowTransportEstablishmentDoesNotSpendTheRendererWaitBudget() {
        val state = DisplayReadiness(null)
        state.attachRenderer(0)
        state.transportConnected(25_000)
        assertFalse(state.rendererTimedOut(54_999))
        assertTrue(state.rendererTimedOut(55_000))
    }
    @Test fun derivedUiGatesNewToolsAndDisplaySwitchingForViewOnlyAndBusySessions() {
        val requested = PermissionSet(controlInput = false, clipboard = true, fileTransfer = true)
        val session = RemoteSession("session", "active", "token", requested, secondary)
        val ui = SessionUi(RemoteDevice("123456789", "PC", "windows", true, ""), session,
            connected = true, rendered = true, displays = catalog,
            tools = SessionToolsUi(channelOpen = true, capabilities = HostCapabilities(clipboard = true, fileTransfer = true, multiMonitor = true)),
            requestedPermissions = requested)
        assertTrue(ui.canSwitchDisplay) // Viewing permission is sufficient; no input permission escalation.
        assertTrue(ui.canClipboard); assertTrue(ui.canUpload)
        assertFalse(ui.inputEnabled)
        val rotating = ui.copy(rendered = false)
        assertTrue(rotating.connected) // Current transfer may continue while renderer ownership changes.
        assertFalse(rotating.canClipboard); assertFalse(rotating.canUpload); assertFalse(rotating.canSwitchDisplay)
        assertFalse(ui.copy(tools = ui.tools.copy(busy = true)).canSwitchDisplay)
        assertFalse(ui.copy(tools = ui.tools.copy(pickingDocument = true)).canSwitchDisplay)
        assertFalse(ui.copy(tools = ui.tools.copy(pickingDocument = true)).canClipboard)
        assertFalse(ui.copy(tools = ui.tools.copy(pickingDocument = true)).canUpload)
        assertEquals(requested, ui.copy(session = session.copy(permissions = PermissionSet(controlInput = false))).requestedPermissions)
    }
}
