package com.a1var0w.safeeat.updater

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.net.URL
import java.security.MessageDigest
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import javax.net.ssl.HttpsURLConnection

class UpdateFileProvider : FileProvider()

@InvokeArg
class DownloadArgs {
  lateinit var url: String
  lateinit var sha256: String
  var size: Long = 0
  var versionCode: Long = 0
}

/**
 * Port of the 1.x updater (apps/mobile/modules/safeeat-updater). Downloads a newer
 * SafeEat APK over HTTPS from the fixed SafeEat host only, checks size, SHA-256,
 * package, version code and that it is signed with the same certificate as the
 * installed app, then opens the Android installer. Android always asks the user:
 * nothing is installed silently.
 *
 * `download` returns at once; `progress` reports the state while it runs, so no call
 * stays open for minutes.
 */
@TauriPlugin
class UpdaterPlugin(private val activity: Activity) : Plugin(activity) {
  private val worker = Executors.newSingleThreadExecutor()
  private val busy = AtomicBoolean(false)
  private val cancelled = AtomicBoolean(false)
  private val received = AtomicLong(0)
  private val total = AtomicLong(0)
  @Volatile private var state = "idle" // idle | downloading | ready | failed
  @Volatile private var message: String? = null
  @Volatile private var connection: HttpsURLConnection? = null
  @Volatile private var verified: Pair<String, Long>? = null

  private val context get() = activity.applicationContext
  private val directory get() = File(context.cacheDir, "safeeat-updates").apply { mkdirs() }
  private val apk get() = File(directory, "SafeEat.apk")

  init {
    // Leftovers of an earlier run are never installed: the next update downloads again.
    worker.execute { directory.listFiles()?.forEach { it.delete() } }
  }

  @Suppress("DEPRECATION")
  private fun code(info: PackageInfo): Long =
    if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong()

  // Android 9/10 only collect archive certificates when GET_SIGNATURES is also set.
  @Suppress("DEPRECATION")
  private val flags
    get() =
      if (Build.VERSION.SDK_INT >= 28)
        PackageManager.GET_SIGNING_CERTIFICATES or PackageManager.GET_SIGNATURES
      else PackageManager.GET_SIGNATURES

  @Suppress("DEPRECATION")
  private fun installedInfo() = context.packageManager.getPackageInfo(context.packageName, flags)

  @Suppress("DEPRECATION")
  private fun signatures(info: PackageInfo): Set<String> {
    val signatures =
      if (Build.VERSION.SDK_INT >= 28) info.signingInfo?.apkContentsSigners else info.signatures
    return signatures?.map { hex(MessageDigest.getInstance("SHA-256").digest(it.toByteArray())) }
      ?.toSet() ?: emptySet()
  }

  private fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it) }

  private fun fileHash(file: File): String {
    val hash = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { stream ->
      val buffer = ByteArray(65536)
      while (true) {
        val n = stream.read(buffer)
        if (n < 0) break
        hash.update(buffer, 0, n)
      }
    }
    return hex(hash.digest())
  }

  private fun validate(file: File, sha256: String, versionCode: Long) {
    require(fileHash(file).equals(sha256, true)) {
      "La descarga no está completa. Vuelve a intentarlo."
    }
    val candidate =
      requireNotNull(context.packageManager.getPackageArchiveInfo(file.path, flags)) {
        "El archivo no es una aplicación válida."
      }
    val current = installedInfo()
    require(candidate.packageName == context.packageName) { "El archivo no corresponde a SafeEat." }
    require(code(candidate) == versionCode && versionCode > code(current)) {
      "La versión descargada no corresponde a esta actualización."
    }
    val mine = signatures(current)
    val theirs = signatures(candidate)
    require(mine.isNotEmpty() && theirs.isNotEmpty()) {
      "No se ha podido comprobar la firma de la actualización. Vuelve a intentarlo."
    }
    require(mine == theirs) { "La firma de la actualización no coincide con SafeEat." }
  }

  /** Only the SafeEat download folder: production releases or preproduction builds. */
  private fun allowed(url: URL) =
    url.protocol == "https" &&
      url.host == "chat-ink.tail552c89.ts.net" &&
      url.port == 10000 &&
      url.userInfo == null &&
      url.query == null &&
      url.ref == null &&
      (Regex("/safeeat-downloads/releases/SafeEat-[0-9]+\\.[0-9]+\\.[0-9]+\\.apk").matches(url.path) ||
        Regex("/safeeat-downloads/SafeEat-[0-9]+\\.[0-9]+\\.[0-9]+-preproduction\\.apk")
          .matches(url.path))

  private fun canInstall() =
    Build.VERSION.SDK_INT < 26 || context.packageManager.canRequestPackageInstalls()

  @Command
  fun installed(invoke: Invoke) {
    val info = installedInfo()
    invoke.resolve(
      JSObject().put("version", info.versionName ?: "").put("versionCode", code(info))
    )
  }

  @Command
  fun download(invoke: Invoke) {
    val args = invoke.parseArgs(DownloadArgs::class.java)
    if (!busy.compareAndSet(false, true)) {
      invoke.resolve()
      return
    }
    cancelled.set(false)
    verified = null
    received.set(0)
    total.set(args.size)
    message = null
    state = "downloading"
    invoke.resolve()
    worker.execute {
      val part = File(directory, "SafeEat.part")
      try {
        val url = URL(args.url)
        require(
          allowed(url) &&
            Regex("[a-fA-F0-9]{64}").matches(args.sha256) &&
            args.size in 100_000..262_144_000 &&
            args.versionCode > 0
        ) {
          "No se puede verificar esta actualización."
        }
        apk.delete()
        part.delete()
        require(directory.usableSpace > args.size * 2) {
          "Libera espacio en el móvil antes de actualizar."
        }
        val conn = url.openConnection() as HttpsURLConnection
        connection = conn
        conn.connectTimeout = 15_000
        conn.readTimeout = 20_000
        conn.instanceFollowRedirects = false
        require(conn.responseCode == 200) { "No se ha podido descargar la actualización." }
        require(conn.contentLengthLong == args.size) {
          "El tamaño de la actualización no coincide."
        }
        conn.inputStream.use { input ->
          part.outputStream().use { output ->
            val buffer = ByteArray(65536)
            while (true) {
              if (cancelled.get()) throw InterruptedException()
              val n = input.read(buffer)
              if (n < 0) break
              val now = received.addAndGet(n.toLong())
              require(now <= args.size) { "El archivo supera el tamaño esperado." }
              output.write(buffer, 0, n)
            }
          }
        }
        if (cancelled.get()) throw InterruptedException()
        require(received.get() == args.size) {
          "La descarga se ha interrumpido. Vuelve a intentarlo."
        }
        validate(part, args.sha256, args.versionCode)
        require(part.renameTo(apk)) { "No se ha podido guardar la actualización." }
        verified = Pair(args.sha256, args.versionCode)
        state = "ready"
      } catch (error: Exception) {
        part.delete()
        apk.delete()
        message =
          when {
            cancelled.get() -> null
            error is java.io.IOException ->
              "La descarga se ha interrumpido. Comprueba tu conexión y vuelve a intentarlo."
            else -> error.message ?: "No se ha podido descargar. Vuelve a intentarlo."
          }
        state = if (cancelled.get()) "idle" else "failed"
      } finally {
        connection?.disconnect()
        connection = null
        busy.set(false)
      }
    }
  }

  @Command
  fun progress(invoke: Invoke) {
    invoke.resolve(
      JSObject()
        .put("state", state)
        .put("received", received.get())
        .put("total", total.get())
        .put("message", message)
    )
  }

  @Command
  fun cancel(invoke: Invoke) {
    cancelled.set(true)
    connection?.disconnect()
    invoke.resolve()
  }

  /** Opens the Android installer, or says that SafeEat may not install apps yet. */
  @Command
  fun install(invoke: Invoke) {
    worker.execute {
      try {
        if (!canInstall()) {
          invoke.resolve(JSObject().put("opened", false).put("needsPermission", true))
          return@execute
        }
        val ready = requireNotNull(verified) { "Descarga la actualización de nuevo." }
        validate(apk, ready.first, ready.second)
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.updates", apk)
        activity.runOnUiThread {
          try {
            val intent =
              Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            intent.clipData = ClipData.newRawUri("SafeEat", uri)
            activity.startActivity(intent)
            invoke.resolve(JSObject().put("opened", true).put("needsPermission", false))
          } catch (error: Exception) {
            invoke.reject("No se ha podido abrir el instalador.")
          }
        }
      } catch (error: Exception) {
        state = "failed"
        message = error.message
        invoke.reject(error.message ?: "No se ha podido instalar la actualización.")
      }
    }
  }

  /** «Instalar aplicaciones desconocidas» for SafeEat (Android 8+). */
  @Command
  fun openSettings(invoke: Invoke) {
    activity.runOnUiThread {
      try {
        if (Build.VERSION.SDK_INT >= 26) {
          activity.startActivity(
            Intent(
              Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
              Uri.parse("package:${context.packageName}"),
            )
          )
        }
        invoke.resolve()
      } catch (error: Exception) {
        invoke.reject("No se han podido abrir los ajustes.")
      }
    }
  }
}
