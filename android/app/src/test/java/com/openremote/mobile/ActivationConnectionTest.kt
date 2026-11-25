package com.openremote.mobile

import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.time.Instant
import java.util.Base64

class ActivationConnectionTest {
    private val info = """{"service":"openremote","protocolVersion":1,"version":"0.8.0","phoneSetupVersion":1,"monitorSelection":true,"pairingCodeVersion":1,"webController":true,"automaticHostRegistrationVersion":1,"activationVersion":1,"maxActivationConnections":4,"relay":{"configured":false,"udp":false,"tcp":false,"tls":false},"relayOnly":false}"""
    private class Signer : ChallengeSigner {
        var proofs = 0
        override fun publicKeyBase64() = "phone-public-key"
        override fun signChallenge(challenge: ByteArray): String { proofs++; return "proof" }
    }
    private fun client(server: MockWebServer, signer: Signer = Signer()) = ApiClient(server.url("/").toString(), signer, allowLoopbackHttpForTest = true)
    private fun auth(server: MockWebServer) {
        server.enqueue(MockResponse().setBody(info))
        server.enqueue(MockResponse().setBody(JSONObject().put("challengeId", "challenge").put("challenge", Base64.getEncoder().encodeToString(ByteArray(32))).toString()))
        server.enqueue(MockResponse().setBody(JSONObject().put("accessToken", "controller").put("expiresAt", Instant.now().plusSeconds(300).toString()).toString()))
    }
    private fun session(pc: String = "000123456", mode: String = "activation", control: Boolean = false) = JSONObject()
        .put("controllerToken", "t".repeat(43)).put("session", JSONObject()
            .put("id", "session_identifier_1234").put("pairingId", "pairing_identifier_1234")
            .put("status", "pending").put("hostDisplayId", pc).put("hostName", "Office PC")
            .put("hostPublicKey", Base64.getEncoder().encodeToString(ByteArray(32)))
            .put("mode", mode).put("permissions", PermissionSet(controlInput = control).toJson()))

    @Test fun permanentPcIdIsAcceptedWithoutChangingSixDigitPinRules() {
        PairingInput.validate("000123456", "001234")
        assertTrue(PairingInput.partialPcId("000123456"))
        assertFalse(PairingInput.partialDigits("000123456"))
        assertFalse(PairingInput.validPin("000123456"))
        listOf("12345678", "1234567890", "123 456 789", "１２３４５６７８９", "000123456\n").forEach {
            assertThrows(IllegalArgumentException::class.java) { PairingInput.validate(it, "001234") }
        }
        val ui = AppUiState(settingsLoaded = true, endpoint = "https://remote.company.net")
        assertEquals(ManagedHomeAction.Connect, managedHomeAction(ui, "000123456", "001234"))
    }

    @Test fun activationDiscoveryIsStrictAndLegacyCompatible() {
        val parsed = ServerInfo.decode(info.toByteArray())
        assertEquals(1, parsed.activationVersion); assertEquals(4, parsed.maxActivationConnections)
        val old = info.replace("\"activationVersion\":1,\"maxActivationConnections\":4,", "")
        assertEquals(0, ServerInfo.decode(old.toByteArray()).activationVersion)
        for (bad in listOf(info.replace("\"activationVersion\":1", "\"activationVersion\":true"),
            info.replace("\"activationVersion\":1", "\"activationVersion\":2"),
            info.replace("\"maxActivationConnections\":4", "\"maxActivationConnections\":0"),
            info.replace("\"maxActivationConnections\":4", "\"maxActivationConnections\":\"4\""),
            info.replace("\"activationVersion\":1,", ""))) {
            assertThrows(IllegalArgumentException::class.java) { ServerInfo.decode(bad.toByteArray()) }
        }
    }

    @Test fun permanentIdNeedsCapabilityBeforeDisclosingPinOrPhoneProof(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); server.enqueue(MockResponse().setBody(info.replace("\"activationVersion\":1,\"maxActivationConnections\":4,", "")))
            val signer = Signer()
            try { client(server, signer).redeemPairing("000123456", "001234", "Android phone"); fail("Legacy server accepted permanent-ID activation") }
            catch (_: PairingNotSupportedException) { }
            assertEquals(1, server.requestCount); assertEquals(0, signer.proofs)
        }
    }

    @Test fun permanentIdReachesAuthenticatedBodyAndPreservesViewOnlyGrant(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); auth(server); server.enqueue(MockResponse().setResponseCode(201).setBody(session().toString()))
            val result = client(server).redeemPairing("000123456", "001234", "Android · Phone")
            assertEquals("000123456", result.device.displayId); assertTrue(result.session.activation)
            assertFalse(result.session.permissions.controlInput)
            repeat(3) { server.takeRequest() }
            val request = server.takeRequest()
            assertEquals("/v1/pairings/redeem", request.path)
            assertEquals("Bearer controller", request.getHeader("Authorization"))
            val body = JSONObject(request.body.readUtf8())
            assertEquals("000123456", body.getString("code")); assertEquals("001234", body.getString("pin"))
            assertEquals("Android · Phone", body.getString("controllerName"))
        }
    }

    @Test fun wrongPcOrWrongModeCleansKnownSessionBeforeRejecting(): Unit = runBlocking {
        for (response in listOf(session(pc = "999999999"), session(mode = "other"), session().apply { getJSONObject("session").remove("mode") })) {
            MockWebServer().use { server ->
                server.start(); auth(server)
                server.enqueue(MockResponse().setResponseCode(201).setBody(response.toString()))
                server.enqueue(MockResponse().setBody("{}"))
                try { client(server).redeemPairing("000123456", "001234", "Phone"); fail("Wrong PC or mode accepted") }
                catch (_: IllegalArgumentException) { }
                repeat(4) { server.takeRequest() }
                assertEquals("/v1/sessions/session_identifier_1234/end", server.takeRequest().path)
                assertEquals(5, server.requestCount)
            }
        }
    }

    @Test fun deviceNamesRemainReadableBoundedAndFreeOfControls() {
        assertEquals("Android · Samsung Phone", controllerDeviceName("Samsung", "Phone"))
        assertEquals("Android", controllerDeviceName("", ""))
        val value = controllerDeviceName("x\u202e\u0000", "🖥".repeat(80))
        assertTrue(value.toByteArray(Charsets.UTF_8).size <= 80)
        assertFalse(value.contains('\u202e')); assertFalse(value.contains('\u0000'))
        assertFalse(value.last().isHighSurrogate())
    }

    @Test fun sessionRefreshCannotSwitchPermanentPcOrMode(): Unit = runBlocking {
        for (mutate in listOf<(JSONObject) -> Unit>({ it.put("hostDisplayId", "999999999") }, { it.remove("mode") })) {
            MockWebServer().use { server ->
                server.start(); auth(server); server.enqueue(MockResponse().setResponseCode(201).setBody(session().toString()))
                val api = client(server)
                val current = api.redeemPairing("000123456", "001234", "Phone").session
                val changed = session().apply { mutate(getJSONObject("session")) }
                server.enqueue(MockResponse().setBody(changed.toString()))
                try { api.sessionStatus(current); fail("Changed PC or mode accepted") }
                catch (_: IllegalArgumentException) { }
            }
        }
    }
}
