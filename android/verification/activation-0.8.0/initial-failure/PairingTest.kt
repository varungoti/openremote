package com.openremote.mobile

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.time.Instant
import java.util.Base64
import java.util.concurrent.TimeUnit

class PairingTest {
    private val sessionId = "session_identifier_1234"
    private val pairingId = "pairing_identifier_1234"
    private val sessionToken = "a".repeat(43)
    private val info = """{"service":"openremote","protocolVersion":1,"version":"0.7.0","phoneSetupVersion":1,"monitorSelection":true,"pairingCodeVersion":1,"webController":true,"automaticHostRegistrationVersion":1,"relay":{"configured":false,"udp":false,"tcp":false,"tls":false},"relayOnly":false}"""
    private class Signer : ChallengeSigner {
        var proofs = 0
        override fun publicKeyBase64() = "test-public-key"
        override fun signChallenge(challenge: ByteArray): String { proofs++; return "test-proof" }
    }
    private fun client(server: MockWebServer, signer: Signer = Signer()) = ApiClient(server.url("/").toString(), signer, allowLoopbackHttpForTest = true)
    private fun auth(server: MockWebServer) {
        server.enqueue(MockResponse().setBody(info))
        server.enqueue(MockResponse().setBody(JSONObject().put("challengeId", "challenge-id").put("challenge", Base64.getEncoder().encodeToString(ByteArray(32))).toString()))
        server.enqueue(MockResponse().setBody(JSONObject().put("accessToken", "controller-access").put("expiresAt", Instant.now().plusSeconds(300).toString()).toString()))
    }
    private fun response(): JSONObject = JSONObject().put("controllerToken", sessionToken).put("session", JSONObject()
        .put("id", sessionId).put("status", "pending").put("hostName", "Office PC").put("hostDisplayId", "123456789")
        .put("hostPublicKey", Base64.getEncoder().encodeToString(ByteArray(32))).put("pairingId", pairingId)
        .put("permissions", PermissionSet().toJson()))
    private fun profile(origin: String = "") = JSONObject().put("schemaVersion", 1).put("serviceName", "OpenRemote").put("apiOrigin", origin).toString().toByteArray()

    @Test fun preservesLeadingZeroesAndRejectsAmbiguousCredentials() {
        PairingInput.validate("000123", "001234")
        listOf("12345", "1234567", "１２３４５６", "123 456", "12-3456", "12345\n").forEach {
            assertThrows(IllegalArgumentException::class.java) { PairingInput.validate(it, "012345") }
        }
        listOf("", "12345", "1".repeat(7), "1".repeat(12), "1".repeat(13), "12345a", " 123456", "１２３４５６").forEach {
            assertThrows(IllegalArgumentException::class.java) { PairingInput.validate("012345", it) }
        }
    }

    @Test fun configuredProfileSuppliesOnlyMissingOriginAndCannotMigrateExistingIdentity() {
        assertEquals("", ServiceProfile.endpoint(profile()))
        assertEquals("https://remote.company.net", ServiceProfile.endpoint(profile("https://remote.company.net/")))
        val saved = UserSettings("https://original.company.net", listOf(SavedPc("123456789", "Office")))
        assertEquals(saved, ServiceProfile.initialSettings(saved, profile("https://replacement.company.net")))
        assertEquals(saved, ServiceProfile.initialSettings(saved, "invalid profile is never consulted for existing settings".toByteArray()))
        assertEquals("https://remote.company.net", ServiceProfile.initialSettings(UserSettings(), profile("https://remote.company.net")).endpoint)
    }

    @Test fun actualBuildProfileDecodesThroughTheProductionStartupParser() {
        val configured = ServiceProfile.initialSettings(UserSettings(), BuildConfig.SERVICE_PROFILE_JSON.toByteArray())
        assertTrue(configured.endpoint.isEmpty() || configured.endpoint.startsWith("https://"))
        assertTrue(configured.savedPcs.isEmpty())
    }

    @Test fun profileRejectsCredentialsUnknownFieldsDuplicateKeysAndAmbiguousOrigins() {
        listOf("http://remote.company.net", "https://u:p@remote.company.net", "https://remote.company.net/a", "https://remote.company.net/a/..",
            "https://remote.company.net?pin=123456", "https://remote.company.net#fragment", "https://remote.company.net\\path", "https://%72emote.company.net",
            " https://remote.company.net", "https://api.example.com", "https://localhost", "https://remote.invalid", "https://remote.company.net:99999").forEach {
            assertThrows(Exception::class.java) { ServiceProfile.endpoint(profile(it)) }
        }
        assertThrows(IllegalArgumentException::class.java) { ServiceProfile.endpoint("""{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"","pin":"123456"}""".toByteArray()) }
        assertThrows(IllegalArgumentException::class.java) { ServiceProfile.endpoint("""{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"","apiOrigin":""}""".toByteArray()) }
    }

    @Test fun codeAndPinReachOnlyAuthenticatedJsonBodyAfterCapabilityCheck(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); auth(server); server.enqueue(MockResponse().setResponseCode(201).setBody(response().toString()))
            val result = client(server).redeemPairing("000123", "001234", "My phone")
            assertEquals("Office PC", result.device.name)
            assertEquals("123456789", result.device.displayId)
            assertEquals(pairingId, result.session.pairingId)
            assertNull(result.session.monitorId)
            assertEquals("/v1/info", server.takeRequest().path)
            assertEquals("/v1/auth/challenge", server.takeRequest().path)
            assertEquals("/v1/auth/verify", server.takeRequest().path)
            val redeem = server.takeRequest()
            assertEquals("/v1/pairings/redeem", redeem.path)
            assertEquals("Bearer controller-access", redeem.getHeader("Authorization"))
            val body = JSONObject(redeem.body.readUtf8())
            assertEquals(setOf("code", "pin", "controllerName"), body.keys().asSequence().toSet())
            assertEquals("000123", body.getString("code")); assertEquals("001234", body.getString("pin"))
            assertFalse(result.toString().contains("001234"))
        }
    }

    @Test fun unsupportedCoordinatorStopsBeforePhoneProofOrPinDisclosure(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); server.enqueue(MockResponse().setBody(info.replace("\"pairingCodeVersion\":1,", "").replace("\"webController\":true,", "")))
            val signer = Signer()
            try { client(server, signer).redeemPairing("000123", "001234", "Phone"); fail("Legacy pairing accepted") }
            catch (_: PairingNotSupportedException) { }
            assertEquals(0, signer.proofs); assertEquals(1, server.requestCount)
        }
    }

    @Test fun invalidInputDoesNotContactCoordinator(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start()
            try { client(server).redeemPairing("123", "001234", "Phone"); fail("Invalid code accepted") }
            catch (_: IllegalArgumentException) { }
            try { client(server).redeemPairing("123456", "0012345", "Phone"); fail("Seven-digit PIN accepted") }
            catch (_: IllegalArgumentException) { }
            assertEquals(0, server.requestCount)
        }
    }

    @Test fun singleUsePostIsNeverReplayedOnRetryableResponse(): Unit = runBlocking {
        for (status in listOf(401, 408, 503)) MockWebServer().use { server ->
            server.start(); auth(server)
            server.enqueue(MockResponse().setResponseCode(status).setHeader("Retry-After", "0").setBody("""{"error":"temporarily_unavailable"}"""))
            try { client(server).redeemPairing("000123", "001234", "Phone"); fail("Failure expected") }
            catch (e: ApiException) { assertEquals(status, e.status) }
            assertEquals(4, server.requestCount)
        }
    }

    @Test fun cancelledInFlightRedemptionEndsKnownLateSession(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); auth(server)
            server.enqueue(MockResponse().setResponseCode(201).setBody(response().toString()).setBodyDelay(1, TimeUnit.SECONDS))
            server.enqueue(MockResponse().setBody("{}"))
            val job = async { client(server).redeemPairing("000123", "001234", "Phone") }
            withContext(Dispatchers.IO) { repeat(4) { assertNotNull(server.takeRequest(5, TimeUnit.SECONDS)) } }
            job.cancelAndJoin()
            assertTrue(job.isCancelled)
            val end = withContext(Dispatchers.IO) { server.takeRequest(5, TimeUnit.SECONDS) }
            assertNotNull(end); assertEquals("/v1/sessions/$sessionId/end", end!!.path)
            assertEquals("Bearer $sessionToken", end.getHeader("Authorization"))
            assertEquals(5, server.requestCount)
        }
    }

    @Test fun unsafeGrantIsRejectedAndKnownSessionIsEnded(): Unit = runBlocking {
        for (mutation in listOf<(JSONObject) -> Unit>(
            { it.getJSONObject("permissions").put("clipboard", true) },
            { it.getJSONObject("permissions").put("power", true) },
            { it.getJSONObject("permissions").put("controlInput", "true") },
            { it.put("monitorId", "a".repeat(64)) },
            { it.put("pairingId", "wrong-short-id") },
            { it.put("status", "approved") },
            { it.put("hostDisplayId", "123 456 789") },
        )) MockWebServer().use { server ->
            server.start(); auth(server)
            val result = response().apply { mutation(getJSONObject("session")) }
            server.enqueue(MockResponse().setResponseCode(201).setBody(result.toString())); server.enqueue(MockResponse().setBody("{}"))
            try { client(server).redeemPairing("000123", "001234", "Phone"); fail("Unsafe grant accepted") }
            catch (_: IllegalArgumentException) { }
            repeat(4) { server.takeRequest() }
            assertEquals("/v1/sessions/$sessionId/end", server.takeRequest().path)
        }
    }

    @Test fun pairingBindingAndPermissionCeilingSurviveSessionPolls(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start()
            val api = client(server)
            val session = RemoteSession(sessionId, "pending", sessionToken, PermissionSet(controlInput = false), pairingId = pairingId)
            val changed = response().apply { getJSONObject("session").put("status", "approved") }
            server.enqueue(MockResponse().setBody(changed.toString()))
            try { api.sessionStatus(session); fail("Input escalation accepted") } catch (_: IllegalArgumentException) { }
            changed.getJSONObject("session").getJSONObject("permissions").put("controlInput", false)
            changed.getJSONObject("session").put("pairingId", "different_pairing_12345")
            server.enqueue(MockResponse().setBody(changed.toString()))
            try { api.sessionStatus(session); fail("Pairing binding changed") } catch (_: IllegalArgumentException) { }
        }
    }

    @Test fun pairingErrorsProvideRecoveryWithoutSecrets() {
        val message = pairingError(ApiException(403, "pairing_unavailable"))
        assertTrue(message.contains("create a new Server ID"))
        assertFalse(message.contains("012345"))
        assertTrue(pairingError(PairingResultUnknownException(java.io.IOException("private payload"))).contains("may have been used"))
        assertFalse(pairingError(PairingResultUnknownException(java.io.IOException("private payload"))).contains("private payload"))
        assertTrue(pairingError(ApiException(429, "pairing_rate_limited")).contains("wait"))
        assertTrue(pairingError(ApiException(503, "storage_unavailable")).contains("may have been used"))
        assertTrue(pairingError(ApiException(408, "request_timeout")).contains("create a new Server ID"))
        assertFalse(pairingError(ApiException(404, "not_found")).contains("PC number"))
    }
}
