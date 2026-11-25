package com.openremote.mobile

import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.net.ConnectException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import java.time.Instant
import java.util.Base64
import java.util.concurrent.TimeUnit
import javax.net.ssl.SSLHandshakeException

class ConnectionChecksTest {
    private fun info(configured: Boolean = true) = """{"service":"openremote","protocolVersion":1,"version":"0.5.0","phoneSetupVersion":1,"monitorSelection":true,"relay":{"configured":$configured,"udp":$configured,"tcp":false,"tls":$configured},"relayOnly":false}"""
    private class Signer : ChallengeSigner {
        var proofs = 0
        override fun publicKeyBase64() = "phone-public-key"
        override fun signChallenge(challenge: ByteArray): String { proofs++; assertEquals(32, challenge.size); return "signature-$proofs" }
    }
    private fun auth(server: MockWebServer) {
        server.enqueue(MockResponse().setBody(JSONObject().put("challengeId", "fresh-challenge").put("challenge", Base64.getEncoder().encodeToString(ByteArray(32))).toString()))
        server.enqueue(MockResponse().setBody(JSONObject().put("accessToken", "private-controller-token").put("expiresAt", Instant.now().plusSeconds(300).toString()).toString()))
    }
    private fun client(server: MockWebServer, signer: Signer) = ApiClient(server.url("/").toString(), signer, allowLoopbackHttpForTest = true)

    @Test fun validServerInformationMeansConfiguredTransportsOnly() {
        val parsed = ServerInfo.decode(info().toByteArray())
        assertEquals("0.5.0", parsed.version)
        assertTrue(parsed.relay.configured)
        assertTrue(parsed.relay.udp)
        assertFalse(parsed.relay.tcp)
        assertTrue(parsed.relay.tls)
        assertFalse(parsed.relayOnly)
    }

    @Test fun newPairingCapabilitiesAreTypedAndExplicitWhileLegacyRemainsDiscoverable() {
        val extended = info().dropLast(1) + ",\"pairingCodeVersion\":1,\"webController\":true}"
        val parsed = ServerInfo.decode(extended.toByteArray())
        assertEquals(1, parsed.pairingCodeVersion)
        assertTrue(parsed.webController)
        assertEquals(0, ServerInfo.decode(info().toByteArray()).pairingCodeVersion)
        listOf(extended.replace("\"pairingCodeVersion\":1", "\"pairingCodeVersion\":\"1\""),
            extended.replace("\"pairingCodeVersion\":1", "\"pairingCodeVersion\":2"),
            extended.replace("\"webController\":true", "\"webController\":1")).forEach {
            assertThrows(IllegalArgumentException::class.java) { ServerInfo.decode(it.toByteArray()) }
        }
    }

    @Test fun managedRegistrationDiscoveryIsOptionalAndStrictlyVersioned() {
        assertEquals(0, ServerInfo.decode(info().toByteArray()).automaticHostRegistrationVersion)
        for (version in 0..1) {
            val body = info().dropLast(1) + ",\"automaticHostRegistrationVersion\":$version}"
            assertEquals(version, ServerInfo.decode(body.toByteArray()).automaticHostRegistrationVersion)
        }
        for (value in listOf("-1", "2", "\"1\"", "true", "null")) {
            val body = info().dropLast(1) + ",\"automaticHostRegistrationVersion\":$value}"
            assertThrows(IllegalArgumentException::class.java) { ServerInfo.decode(body.toByteArray()) }
        }
    }

    @Test fun rejectsDuplicateAmbiguousIncompatibleAndOversizedServerInformation() {
        val body = info()
        val invalid = listOf(body.replace("\"service\":", "\"service\":\"openremote\",\"service\":"),
            body.replace("\"openremote\"", "\"other-service\""), body.replace("\"protocolVersion\":1", "\"protocolVersion\":2"),
            body.replace("\"phoneSetupVersion\":1", "\"phoneSetupVersion\":\"1\""), body.replace("\"monitorSelection\":true", "\"monitorSelection\":null"),
            body.replace("\"udp\":true", "\"udp\":\"true\""), body.replace("\"configured\":true", "\"configured\":false"),
            body.dropLast(1) + ",\"token\":\"secret\"}", body.replace("\"0.5.0\"", "\"bad\\nversion\""),
            body.replace("\"0.5.0\"", "\"${"v".repeat(65)}\""), " ".repeat(16385) + body,
            info(false).replace("\"relayOnly\":false", "\"relayOnly\":true"))
        invalid.forEach { bytes -> assertThrows(IllegalArgumentException::class.java) { ServerInfo.decode(bytes.toByteArray()) } }
    }

    @Test fun unrecognizedOrLegacyEndpointStopsBeforeIdentityDisclosure(): Unit = runBlocking {
        for (response in listOf(MockResponse().setResponseCode(404).setBody("<html>secret-server-log</html>"),
            MockResponse().setBody(info().replace("openremote", "another-service")),
            MockResponse().setResponseCode(302).setHeader("Location", "https://other.example/private-token"))) {
            MockWebServer().use { server ->
                server.start(); server.enqueue(response)
                val signer = Signer()
                val result = ConnectionChecks.run(client(server, signer), "https://remote.example", "123456789") { }
                assertEquals(0, signer.proofs)
                assertEquals(1, server.requestCount)
                val request = server.takeRequest()
                assertEquals("/v1/info", request.path)
                assertNull(request.getHeader("Authorization"))
                assertEquals(CheckStatus.FAILED, result.report.checks.single().status)
                assertFalse(result.report.running)
                assertFalse(result.report.asText().contains("secret-server-log"))
                assertFalse(result.report.asText().contains("private-token"))
            }
        }
    }

    @Test fun explicitChecksUseFreshProofAndAuthenticatedPcLookupEachTime(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start()
            val signer = Signer()
            val api = client(server, signer)
            repeat(2) { run ->
                server.enqueue(MockResponse().setBody(info())); auth(server)
                server.enqueue(MockResponse().setBody("""{"displayId":"123456789","name":"Office","platform":"windows","online":true,"lastSeen":"now"}"""))
                val published = mutableListOf<ConnectionCheckReport>()
                val result = ConnectionChecks.run(api, "https://remote.example", "123456789") { published += it }
                assertEquals(run + 1, signer.proofs)
                assertTrue(result.device!!.online)
                assertEquals(listOf("Server", "Internet relay configuration", "Phone identity", "PC availability"), result.report.checks.map { it.title })
                assertEquals(CheckStatus.INFO, result.report.checks[1].status)
                assertEquals(CheckStatus.PASSED, result.report.checks.last().status)
                assertFalse(result.report.running)
                assertTrue(published.first().running)
                assertFalse(result.report.asText().contains("private-controller-token"))
                assertEquals("/v1/info", server.takeRequest().path)
                assertEquals("/v1/auth/challenge", server.takeRequest().path)
                assertEquals("/v1/auth/verify", server.takeRequest().path)
                val lookup = server.takeRequest()
                assertEquals("/v1/devices/123456789", lookup.path)
                assertEquals("Bearer private-controller-token", lookup.getHeader("Authorization"))
            }
        }
    }

    @Test fun offlinePcAndMissingRelayRemainWarningsWithConcreteNextSteps(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); server.enqueue(MockResponse().setBody(info(false))); auth(server)
            server.enqueue(MockResponse().setBody("""{"displayId":"123456789","name":"Office","platform":"windows","online":false,"lastSeen":"now"}"""))
            val result = ConnectionChecks.run(client(server, Signer()), "https://remote.example", "123456789") { }
            assertEquals(CheckStatus.WARNING, result.report.checks[1].status)
            assertEquals(CheckStatus.WARNING, result.report.checks.last().status)
            assertFalse(result.device!!.online)
            assertTrue(result.report.asText().contains("Configure TURN"))
            assertTrue(result.report.asText().contains("Wake it"))
        }
    }

    @Test fun serverOnlyCheckDoesNotSendAnyPcRequest(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); server.enqueue(MockResponse().setBody(info())); auth(server)
            val result = ConnectionChecks.run(client(server, Signer()), "https://remote.example", null) { }
            assertEquals(3, server.requestCount)
            assertNull(result.device)
            assertEquals(CheckStatus.INFO, result.report.checks.last().status)
        }
    }

    @Test fun lookupMustReturnTheRequestedPcAndCannotRedirectTheConnection(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); server.enqueue(MockResponse().setBody(info())); auth(server)
            server.enqueue(MockResponse().setBody("""{"displayId":"987654321","name":"Other PC","platform":"windows","online":true,"lastSeen":"now"}"""))
            val result = ConnectionChecks.run(client(server, Signer()), "https://remote.example", "123456789") { }
            assertNull(result.device)
            assertEquals(CheckStatus.FAILED, result.report.checks.last().status)
        }
    }

    @Test fun cancellationDoesNotContinueToPhoneProofOrPcLookup(): Unit = runBlocking {
        MockWebServer().use { server ->
            server.start(); server.enqueue(MockResponse().setBody(info()).setHeadersDelay(1, TimeUnit.SECONDS))
            val signer = Signer()
            val job = async { ConnectionChecks.run(client(server, signer), "https://remote.example", "123456789") { } }
            // Avoid blocking this runBlocking dispatcher while the request is being launched.
            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { assertNotNull(server.takeRequest(5, TimeUnit.SECONDS)) }
            job.cancelAndJoin()
            assertTrue(job.isCancelled)
            assertEquals(0, signer.proofs)
            assertEquals(1, server.requestCount)
        }
    }

    @Test fun errorsDescribeRecoveryWithoutEchoingLowLevelSecrets() {
        val errors = listOf(UnknownHostException("secret"), SSLHandshakeException("secret"), SocketTimeoutException("secret"), ConnectException("secret"), ApiException(503, "secret"))
        errors.forEach { assertFalse(connectionError(it).contains("secret")) }
        assertTrue(connectionError(UnknownHostException()).contains("DNS"))
        assertTrue(connectionError(SSLHandshakeException("bad")).contains("certificate"))
        assertTrue(connectionError(ApiException(429, "rate_limit")).contains("Wait"))
        assertTrue(connectionError(ApiException(404, "not_found"), "Server").contains("upgrade"))
        assertTrue(connectionError(ApiException(404, "not_found"), "PC availability").contains("PC number"))
    }
}
