package com.openremote.mobile

import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.security.KeyPairGenerator
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.time.Instant
import java.util.Base64
import java.util.concurrent.TimeUnit

class ApiClientTest {
    private val keys = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
    private val signer = object : ChallengeSigner {
        override fun publicKeyBase64() = Base64.getEncoder().encodeToString(keys.public.encoded)
        override fun signChallenge(challenge: ByteArray): String = Base64.getEncoder().encodeToString(Signature.getInstance("SHA256withECDSA").run {
            initSign(keys.private); update(challenge); sign()
        })
    }
    private fun enqueueAuthentication(server: MockWebServer) {
        server.enqueue(MockResponse().setBody(JSONObject().put("challengeId", "challenge1").put("challenge", Base64.getEncoder().encodeToString(ByteArray(32))).toString()))
        server.enqueue(MockResponse().setBody(JSONObject().put("accessToken", "controller-access").put("expiresAt", Instant.now().plusSeconds(300).toString()).toString()))
    }
    private fun createdSession(monitorId: String? = null): JSONObject = JSONObject().put("controllerToken", "session-token").put("session", JSONObject()
        .put("id", "session1").put("status", "pending").put("permissions", PermissionSet().toJson()).apply { monitorId?.let { put("monitorId", it) } })
    @Test fun endpointAndPcIdRejectCredentialLeaksAndAmbiguity() {
        listOf("http://host.example", "https://name:password@host.example", "https://host.example/path", "https://host.example?token=a", "https://host.example#fragment").forEach {
            assertThrows(IllegalArgumentException::class.java) { ApiClient.validateEndpoint(it) }
        }
        assertEquals("123456789", ApiClient.normalizePcId("123-456 789"))
        assertThrows(IllegalArgumentException::class.java) { ApiClient.normalizePcId("a123456789") }
        assertThrows(IllegalArgumentException::class.java) { ApiClient.normalizePcId("1234567890") }
    }
    @Test fun authenticationSignsTheActualChallengeAndNeverSelfAssertsTrust(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start()
            val nonce = ByteArray(32) { it.toByte() }
            server.enqueue(MockResponse().setBody(JSONObject().put("challengeId", "challenge1").put("challenge", Base64.getEncoder().encodeToString(nonce)).put("expiresAt", Instant.now().plusSeconds(60).toString()).toString()))
            server.enqueue(MockResponse().setBody(JSONObject().put("accessToken", "controller-access").put("expiresAt", Instant.now().plusSeconds(300).toString()).toString()))
            server.enqueue(MockResponse().setBody("""{"displayId":"123456789","name":"Office","platform":"windows","online":true,"lastSeen":"now"}"""))
            val client = ApiClient(server.url("/").toString(), signer, allowLoopbackHttpForTest = true)
            val device = client.lookup("123456789")
            assertTrue(device.online)
            val challengeRequest = server.takeRequest()
            assertEquals("/v1/auth/challenge", challengeRequest.path)
            assertNull(challengeRequest.getHeader("Authorization"))
            val proof = JSONObject(server.takeRequest().body.readUtf8())
            assertEquals("challenge1", proof.getString("challengeId"))
            assertTrue(Signature.getInstance("SHA256withECDSA").run {
                initVerify(keys.public); update(nonce); verify(Base64.getDecoder().decode(proof.getString("signature")))
            })
            val lookup = server.takeRequest()
            assertEquals("Bearer controller-access", lookup.getHeader("Authorization"))
            assertEquals("/v1/devices/123456789", lookup.path)
        }
    }
    @Test fun pendingSessionSignalsCannotBorrowControllerIdentity(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start()
            server.enqueue(MockResponse().setResponseCode(409).setBody("""{"error":"invalid_state"}"""))
            val client = ApiClient(server.url("/").toString(), signer, allowLoopbackHttpForTest = true)
            val session = RemoteSession("session1", "pending", "session-token", PermissionSet())
            try { client.sendSignal(session, "offer", JSONObject().put("type", "offer").put("sdp", "test")); fail("Expected server state failure") }
            catch (e: ApiException) { assertEquals(409, e.status) }
            val r = server.takeRequest()
            assertEquals("Bearer session-token", r.getHeader("Authorization"))
            assertEquals("/v1/sessions/session1/signal", r.path)
        }
    }
    @Test fun longPollingIsCancelledWithSession(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start()
            server.enqueue(MockResponse().setBody("""{"signals":[]}""").setHeadersDelay(2, TimeUnit.SECONDS))
            val client = ApiClient(server.url("/").toString(), signer, allowLoopbackHttpForTest = true)
            val job = async { client.pollSignals(RemoteSession("s", "approved", "token", PermissionSet()), 0) }
            delay(100)
            job.cancelAndJoin()
            assertTrue(job.isCancelled)
        }
    }
    @Test fun selectedMonitorIsExplicitScopedAndImmutableAcrossStatusChecks(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); enqueueAuthentication(server)
            val id = "a".repeat(64)
            server.enqueue(MockResponse().setBody(createdSession(id).toString()))
            val client = ApiClient(server.url("/").toString(), signer, allowLoopbackHttpForTest = true)
            val session = client.createSession("123456789", "Phone", PermissionSet(controlInput = false), id)
            assertEquals(id, session.monitorId)
            server.takeRequest(); server.takeRequest()
            val request = server.takeRequest()
            assertEquals("Bearer controller-access", request.getHeader("Authorization"))
            val json = JSONObject(request.body.readUtf8())
            assertEquals(id, json.getString("monitorId"))
            assertFalse(json.getJSONObject("permissions").getBoolean("controlInput"))
            server.enqueue(MockResponse().setBody(createdSession("b".repeat(64)).toString()))
            try { client.sessionStatus(session); fail("Changed display binding accepted") }
            catch (_: IllegalArgumentException) { }
            assertEquals("Bearer session-token", server.takeRequest().getHeader("Authorization"))
        }
    }
    @Test fun missingMonitorEchoEndsLegacyRequestWithoutFallingBack(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); enqueueAuthentication(server)
            server.enqueue(MockResponse().setBody(createdSession().toString())) // Legacy coordinator silently omitted the requested binding.
            server.enqueue(MockResponse().setBody("{}"))
            val client = ApiClient(server.url("/").toString(), signer, allowLoopbackHttpForTest = true)
            try { client.createSession("123456789", "Phone", PermissionSet(), "a".repeat(64)); fail("Missing monitor echo accepted") }
            catch (_: IllegalArgumentException) { }
            server.takeRequest(); server.takeRequest(); server.takeRequest()
            val end = server.takeRequest()
            assertEquals("/v1/sessions/session1/end", end.path)
            assertEquals("Bearer session-token", end.getHeader("Authorization"))
            assertEquals(4, server.requestCount) // No primary-display fallback request.
        }
    }
    @Test fun primarySessionOmitsMonitorFieldAndMalformedSelectionsNeverReachServer(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); enqueueAuthentication(server)
            server.enqueue(MockResponse().setBody(createdSession().toString()))
            val client = ApiClient(server.url("/").toString(), signer, allowLoopbackHttpForTest = true)
            for (invalid in listOf("", "A".repeat(64), "a".repeat(63), "../monitor")) {
                try { client.createSession("123456789", "Phone", PermissionSet(), invalid); fail("Invalid monitor selection accepted") }
                catch (_: IllegalArgumentException) { }
            }
            assertEquals(0, server.requestCount)
            assertNull(client.createSession("123456789", "Phone", PermissionSet()).monitorId)
            server.takeRequest(); server.takeRequest()
            assertFalse(JSONObject(server.takeRequest().body.readUtf8()).has("monitorId"))
        }
    }
}
