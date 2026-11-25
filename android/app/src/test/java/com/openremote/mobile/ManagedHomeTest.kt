package com.openremote.mobile

import org.junit.Assert.*
import org.junit.Test
import java.net.ConnectException
import java.net.UnknownHostException
import javax.net.ssl.SSLHandshakeException

class ManagedHomeTest {
    private val profile = """{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"https://remote.company.net"}""".toByteArray()
    private val noService = """{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":""}""".toByteArray()

    @Test fun freshUnconfiguredCopyCannotSubmitEvenCorrectCredentials() {
        val settings = ServiceProfile.initialSettings(UserSettings(), noService)
        assertEquals("", settings.endpoint)
        val ui = AppUiState(settingsLoaded = true, endpoint = settings.endpoint)
        assertEquals(ManagedHomeAction.Disabled, managedHomeAction(ui, "012345", "001234"))
        assertFalse(managedHomeAction(ui, "012345", "001234").enabled)
    }

    @Test fun configuredCopyCanConnectWithoutAnySetupAction() {
        val settings = ServiceProfile.initialSettings(UserSettings(), profile)
        val ui = AppUiState(settingsLoaded = true, endpoint = settings.endpoint)
        assertEquals(ManagedHomeAction.Connect, managedHomeAction(ui, "012345", "001234"))
        assertEquals(ManagedHomeAction.Disabled, managedHomeAction(ui.copy(settingsLoaded = false), "012345", "001234"))
        assertEquals(ManagedHomeAction.Disabled, managedHomeAction(ui.copy(settingsSaving = true), "012345", "001234"))
    }

    @Test fun oneCancelActionStaysAvailableAfterPinClearsAndUiLocks() {
        // A submitted PIN is immediately cleared; Cancel must remain usable despite
        // that invalid input and even while a concurrent settings write settles.
        val waiting = AppUiState(settingsLoaded = true, endpoint = "https://remote.company.net", busy = true, settingsSaving = true)
        assertTrue(waiting.homeLocked)
        assertEquals(ManagedHomeAction.Cancel, managedHomeAction(waiting, "012345", ""))
        assertTrue(managedHomeAction(waiting, "", "").enabled)
        val cancelled = waiting.copy(busy = false, settingsSaving = false)
        assertEquals(ManagedHomeAction.Disabled, managedHomeAction(cancelled, "012345", ""))
        assertEquals(ManagedHomeAction.Connect, managedHomeAction(cancelled, "012345", "001234"))
    }

    @Test fun clientRejectsEveryLegacyLongPinWithoutUsingItsSixDigitPrefix() {
        val ui = AppUiState(settingsLoaded = true, endpoint = "https://remote.company.net")
        for (length in 7..12) {
            val pin = "0".repeat(length)
            assertFalse(PairingInput.validPin(pin))
            assertFalse(PairingInput.partialDigits(pin))
            assertEquals(ManagedHomeAction.Disabled, managedHomeAction(ui, "012345", pin))
            assertThrows(IllegalArgumentException::class.java) { PairingInput.validate("012345", pin) }
        }
        for (value in listOf(" 12345", "12345\n", "１２３４５６", "1234567")) {
            assertEquals(ManagedHomeAction.Disabled, managedHomeAction(ui, value, "001234"))
            assertEquals(ManagedHomeAction.Disabled, managedHomeAction(ui, "012345", value))
        }
    }

    @Test fun previouslyAuthorizedOriginIsRetainedButUnsafeHiddenSettingsAreRejected() {
        val saved = UserSettings("https://old.company.net", listOf(SavedPc("123456789", "Office")))
        assertEquals(saved, ServiceProfile.initialSettings(saved, profile))
        assertEquals(saved, ServiceProfile.initialSettings(saved, noService))
        for (origin in listOf("http://remote.company.net", "https://user:secret@remote.company.net", "https://remote.company.net?pin=secret", "https://remote.company.net/a/..", "https://api.example.com")) {
            assertThrows(Exception::class.java) { ServiceProfile.initialSettings(UserSettings(origin), profile) }
        }
    }

    @Test fun currentRecoveryNeverAsksUsersToConfigureServersOrDisclosesPrivateDetails() {
        val errors = listOf(UnknownHostException("private-service"), SSLHandshakeException("private-certificate"),
            ConnectException("private-address"), ApiException(401, "private_token"), PairingNotSupportedException(),
            PairingResultUnknownException(java.io.IOException("private-response")), ApiException(404, "not_found"))
        val messages = errors.map(::pairingError) + listOf(ManagedClientText.UNAVAILABLE, ManagedClientText.STARTUP_UNAVAILABLE, ManagedClientText.SESSION_STOPPED)
        for (message in messages) {
            assertFalse(message.contains("private"))
            assertFalse(message.contains("https://"))
            assertFalse(message.contains("Settings"))
            assertFalse(message.contains("Advanced"))
            assertFalse(message.contains("DNS"))
            assertFalse(message.contains("coordinator"))
            assertFalse(message.contains("certificate"))
            assertFalse(message.contains("fingerprint"))
        }
    }
}
