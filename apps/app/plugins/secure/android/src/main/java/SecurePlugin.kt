package com.a1var0w.safeeat.secure

import android.app.Activity
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONObject

@InvokeArg
class KeyArgs {
  lateinit var key: String
}

@InvokeArg
class SetArgs {
  lateinit var key: String
  lateinit var value: String
}

/**
 * Small secret store. Values are encrypted with an AES-256-GCM key that lives in the
 * Android Keystore (inside secure hardware when the device has it); only the
 * ciphertext is written to the private preferences of the app.
 */
@TauriPlugin
class SecurePlugin(private val activity: Activity) : Plugin(activity) {
  private val prefs
    get() = activity.getSharedPreferences("safeeat_secure", Context.MODE_PRIVATE)

  private fun secretKey(): SecretKey {
    val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
    (store.getEntry(ALIAS, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
    generator.init(
      KeyGenParameterSpec.Builder(
          ALIAS,
          KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
        .setKeySize(256)
        .build()
    )
    return generator.generateKey()
  }

  @Command
  fun set(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SetArgs::class.java)
      val cipher = Cipher.getInstance(TRANSFORMATION)
      cipher.init(Cipher.ENCRYPT_MODE, secretKey())
      val sealed = cipher.iv + cipher.doFinal(args.value.toByteArray(Charsets.UTF_8))
      prefs.edit().putString(args.key, Base64.encodeToString(sealed, Base64.NO_WRAP)).commit()
      invoke.resolve()
    } catch (error: Exception) {
      invoke.reject("No se pudo guardar de forma segura")
    }
  }

  @Command
  fun get(invoke: Invoke) {
    val args = invoke.parseArgs(KeyArgs::class.java)
    val result = JSObject()
    val stored = prefs.getString(args.key, null)
    var value: String? = null
    if (stored != null) {
      try {
        val sealed = Base64.decode(stored, Base64.NO_WRAP)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(128, sealed, 0, IV_BYTES))
        value = String(cipher.doFinal(sealed, IV_BYTES, sealed.size - IV_BYTES), Charsets.UTF_8)
      } catch (error: Exception) {
        // Key lost (restored backup, cleared keystore): the entry is useless, drop it.
        prefs.edit().remove(args.key).commit()
      }
    }
    result.put("value", value ?: JSONObject.NULL)
    invoke.resolve(result)
  }

  @Command
  fun remove(invoke: Invoke) {
    val args = invoke.parseArgs(KeyArgs::class.java)
    prefs.edit().remove(args.key).commit()
    invoke.resolve()
  }

  companion object {
    private const val KEYSTORE = "AndroidKeyStore"
    private const val ALIAS = "safeeat_session"
    private const val TRANSFORMATION = "AES/GCM/NoPadding"
    private const val IV_BYTES = 12
  }
}
