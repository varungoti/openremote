package com.openremote.mobile

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.KeyStore
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Small private metadata file, AES-GCM encrypted with a non-exportable Keystore key. */
class SecureSettings(context: Context) {
    private val file = AtomicFile(File(context.noBackupFilesDir, "settings.enc"))
    private val alias = "openremote_settings_aes_v1"
    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        if (!ks.containsAlias(alias)) KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        }.generateKey()
        return ks.getKey(alias, null) as SecretKey
    }
    @Synchronized fun load(): UserSettings {
        if (!file.baseFile.exists()) return UserSettings()
        val envelope = JSONObject(file.openRead().use { it.bufferedReader().readText() })
        val iv = Base64.getDecoder().decode(envelope.getString("iv"))
        val bytes = Base64.getDecoder().decode(envelope.getString("data"))
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv)) }
        val j = JSONObject(String(cipher.doFinal(bytes), Charsets.UTF_8))
        val list = j.getJSONArray("pcs")
        return UserSettings(j.getString("endpoint"), (0 until list.length()).map { i ->
            val p = list.getJSONObject(i); SavedPc(p.getString("displayId"), p.getString("name"))
        }.take(100))
    }
    @Synchronized fun save(settings: UserSettings) {
        val pcs = JSONArray().apply { settings.savedPcs.take(100).forEach { put(JSONObject().put("displayId", it.displayId).put("name", it.name)) } }
        val plain = JSONObject().put("endpoint", settings.endpoint).put("pcs", pcs).toString().toByteArray()
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val encoded = JSONObject().put("iv", Base64.getEncoder().encodeToString(cipher.iv))
            .put("data", Base64.getEncoder().encodeToString(cipher.doFinal(plain))).toString().toByteArray()
        val out = file.startWrite()
        try { out.write(encoded); file.finishWrite(out) } catch (e: Exception) { file.failWrite(out); throw e }
    }
}
