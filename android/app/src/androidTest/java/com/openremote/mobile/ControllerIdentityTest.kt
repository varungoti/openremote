package com.openremote.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.security.KeyFactory
import java.security.Signature
import java.security.spec.X509EncodedKeySpec
import java.util.Base64

@RunWith(AndroidJUnit4::class)
class ControllerIdentityTest {
    @Test fun keystoreProofPersistsAndVerifiesWithoutExportingPrivateKey() {
        val first = ControllerIdentity()
        val second = ControllerIdentity()
        assertEquals(first.publicKeyBase64(), second.publicKeyBase64())
        val nonce = ByteArray(32) { (it * 3).toByte() }
        val key = KeyFactory.getInstance("EC").generatePublic(X509EncodedKeySpec(Base64.getDecoder().decode(first.publicKeyBase64())))
        assertTrue(Signature.getInstance("SHA256withECDSA").run {
            initVerify(key); update(nonce); verify(Base64.getDecoder().decode(first.signChallenge(nonce)))
        })
    }
}
