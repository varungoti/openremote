package com.openremote.mobile

import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.MessageDigest
import java.security.PrivateKey
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.util.Base64

interface ChallengeSigner {
    fun publicKeyBase64(): String
    fun signChallenge(challenge: ByteArray): String
}

/** Private key never leaves AndroidKeyStore. Tokens are never written to disk. */
class ControllerIdentity : ChallengeSigner {
    private val alias = "openremote_controller_p256_v1"
    private fun keyStore(): KeyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    @Synchronized
    private fun ensureKey(): KeyStore {
        val store = keyStore()
        if (!store.containsAlias(alias)) {
            KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore").apply {
                initialize(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY)
                    .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                    .setDigests(KeyProperties.DIGEST_SHA256).build())
            }.generateKeyPair()
        }
        return store
    }
    override fun publicKeyBase64(): String = Base64.getEncoder().encodeToString(ensureKey().getCertificate(alias).publicKey.encoded)
    override fun signChallenge(challenge: ByteArray): String {
        require(challenge.size in 16..512) { "Invalid authentication challenge" }
        val signature = Signature.getInstance("SHA256withECDSA").apply {
            initSign(ensureKey().getKey(alias, null) as PrivateKey)
            update(challenge)
        }.sign()
        return Base64.getEncoder().encodeToString(signature)
    }
    fun fingerprint(): String = MessageDigest.getInstance("SHA-256")
        .digest(Base64.getDecoder().decode(publicKeyBase64()))
        .joinToString("") { "%02x".format(it) }.chunked(4).joinToString(" ")
    fun label(): String = Build.MODEL.take(80)
}
