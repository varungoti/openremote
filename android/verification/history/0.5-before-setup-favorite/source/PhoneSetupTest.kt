package com.openremote.mobile

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import java.net.URLEncoder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class PhoneSetupTest {
    private val valid = "openremote://connect?v=1&server=https%3A%2F%2Fremote.example.com&pc=123456789"
    private fun link(server: String) = "openremote://connect?v=1&server=${URLEncoder.encode(server, "UTF-8")}&pc=123456789"

    @Test fun decodesExactWindowsVectorsWithoutSecretsOrPermissions() {
        assertEquals(PhoneSetup("https://remote.example.com", "123456789"), PhoneSetupLink.parse(valid))
        assertEquals(PhoneSetup("https://remote.example.com:8443", "123456789"), PhoneSetupLink.parse("openremote://connect?v=1&server=https%3A%2F%2Fremote.example.com%3A8443&pc=123456789"))
        assertEquals(PhoneSetup("https://[2001:db8::1]:8443", "123456789"), PhoneSetupLink.parse("openremote://connect?v=1&server=https%3A%2F%2F%5B2001%3Adb8%3A%3A1%5D%3A8443&pc=123456789"))
        assertEquals(PhoneSetup("https://remote.example.com", "123456789"), PhoneSetupLink.parse(link("https://REMOTE.Example.Com:443/")))
        assertEquals("https://xn--bcher-kva.example", PhoneSetupLink.parse(link("https://xn--bcher-kva.example")).endpoint)
    }

    @Test fun rejectsAmbiguousOuterUrlsFieldsAndEncoding() {
        val bad = listOf(
            " $valid", "$valid\n", "$valid#x", valid.replace("connect?", "connect/?"),
            valid.replace("connect?", "user@connect?"), valid.replace("connect?", "connect:443?"),
            valid.replace("openremote:", "https:"), valid.replace("connect?", "other?"),
            "$valid&token=secret", "$valid&pc=987654321", "$valid&%70c=123456789", "$valid&", "$valid&=x",
            valid.replace("v=1", "v=2"), valid.replace("v=1", "v="), valid.replace("v=1", "v=1=1"),
            valid.replace("123456789", "123-456-789"), valid.replace("123456789", "12345678"),
            valid.replace("123456789", "１２３４５６７８９"), valid.replace("123456789", "%C0%AF"),
            valid.replace("123456789", "%ED%A0%80"), valid.replace("123456789", "%00"),
            valid.replace("123456789", "12345678%"), valid.replace("123456789", "12345678%2"),
            valid.replace("123456789", "12345678%GG"), valid.replace("server=", "server=+"),
            "openremote://connect?pc=123456789", "a".repeat(2049),
        )
        for (value in bad) assertThrows("Accepted malformed setup: $value", IllegalArgumentException::class.java) { PhoneSetupLink.parse(value) }
    }

    @Test fun rawOriginsCannotLoseCredentialsOrPathsDuringHttpNormalization() {
        val bad = listOf("http://remote.example.com", "https:remote.example.com", "https:////remote.example.com",
            "https://remote.example.com/a/..", "https://remote.example.com/./", "https://remote.example.com/path",
            "https://user:secret@remote.example.com", "https://remote.example.com?token=secret", "https://remote.example.com#fragment",
            "https://remote.example.com\\", "https://remote.example.com/%2e", "https://%72emote.example.com", "https://bücher.example", "https://remote.example.com ")
        bad.forEach { value -> assertThrows("Accepted non-origin: $value", IllegalArgumentException::class.java) { PhoneSetupLink.parse(link(value)) } }
    }

    @Test fun initialLoadAndIncomingLinksOnlyStageAnExplicitReview() {
        val inbox = SetupInbox()
        val reviewed = inbox.stage(valid, blocked = false)
        assertEquals(reviewed, inbox.pending)
        assertThrows(IllegalArgumentException::class.java) { inbox.approved(reviewed, ready = false) }
        assertThrows(IllegalArgumentException::class.java) { inbox.approved(reviewed.copy(pcId = "987654321"), ready = true) }
        assertThrows(IllegalArgumentException::class.java) { inbox.stage(valid.replace("123456789", "987654321"), blocked = false) }
        assertThrows(IllegalArgumentException::class.java) { inbox.stage(valid, blocked = true) }
        assertEquals(reviewed, inbox.stage(valid, blocked = false)) // duplicate delivery does not replace a review
        assertEquals(reviewed, inbox.approved(reviewed, ready = true))
        assertEquals(reviewed, inbox.pending) // failed persistence can retain the exact review
        inbox.dismiss()
        assertThrows(IllegalArgumentException::class.java) { inbox.approved(reviewed, ready = true) }
    }

    @Test fun settingsLoadingSavingAndPendingReviewDisableHomeActions() {
        val ready = AppUiState(settingsLoaded = true)
        assertFalse(ready.homeLocked)
        assertTrue(AppUiState().homeLocked)
        assertTrue(ready.copy(settingsSaving = true).homeLocked)
        assertTrue(ready.copy(busy = true).homeLocked)
        assertTrue(ready.copy(pendingSetup = PhoneSetupLink.parse(valid)).homeLocked)
        val pending = ready.copy(pendingSetup = PhoneSetupLink.parse(valid))
        assertEquals(pending.pendingSetup, pending.copy(endpoint = "https://existing.example", settingsLoaded = true).pendingSetup)
    }

    @Test fun cancelledDiskWriteFinishesBeforeAnotherServerCanBeSaved(): Unit = runBlocking {
        val started = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        val saved = mutableListOf<UserSettings>()
        val old = UserSettings("https://old.example", listOf(SavedPc("123456789", "Old PC")))
        val replacement = UserSettings("https://new.example")
        val transactions = SettingsTransactions { snapshot ->
            if (snapshot == old) { started.complete(Unit); check(release.await(10, TimeUnit.SECONDS)) }
            synchronized(saved) { saved += snapshot }
        }
        val reserved = transactions.reserve(old)
        assertTrue(transactions.busy) // held before its coroutine even starts
        val job = async { reserved.commit() }
        try {
            withTimeout(5_000) { started.await() }
            job.cancel()
            assertTrue(transactions.busy)
            assertThrows(IllegalStateException::class.java) { transactions.reserve(replacement) }
        } finally { release.countDown(); job.cancelAndJoin() }
        assertFalse(transactions.busy)
        transactions.reserve(replacement).commit()
        assertEquals(listOf(old, replacement), saved)
        assertFalse(transactions.busy)
    }

    @Test fun retiredLookupCannotPublishIntoTheNextServerOrClearItsBusyState() {
        val epoch = HomeRequestEpoch()
        val old = epoch.next()
        epoch.invalidate() // cancellation or server change
        val fresh = epoch.next()
        var state = AppUiState(settingsLoaded = true, endpoint = "https://new.example", busy = true)
        if (epoch.current(old)) state = state.copy(busy = false, endpoint = "https://old.example")
        assertEquals("https://new.example", state.endpoint)
        assertTrue(state.busy)
        if (epoch.current(fresh)) state = state.copy(busy = false)
        assertFalse(state.busy)
    }
}
