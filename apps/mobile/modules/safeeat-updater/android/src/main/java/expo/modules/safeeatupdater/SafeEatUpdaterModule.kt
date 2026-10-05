package expo.modules.safeeatupdater

import android.content.Intent
import android.content.ClipData
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import expo.modules.kotlin.Promise
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import expo.modules.kotlin.functions.Queues
import java.io.File
import java.net.URL
import javax.net.ssl.HttpsURLConnection
import java.security.MessageDigest
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

class UpdateFileProvider : FileProvider()

class SafeEatUpdaterModule : Module() {
  private val worker = Executors.newSingleThreadExecutor()
  private val busy = AtomicBoolean(false)
  private val cancelled = AtomicBoolean(false)
  @Volatile private var connection: HttpsURLConnection? = null
  private var verified: Triple<String, String, Long>? = null
  private val context get() = requireNotNull(appContext.reactContext)
  private val directory get() = File(context.cacheDir, "safeeat-updates").apply { mkdirs() }
  private val apk get() = File(directory, "SafeEat.apk")
  @Suppress("DEPRECATION")
  private fun code(info: PackageInfo): Long = if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong()
  private val flags get() = if (Build.VERSION.SDK_INT >= 28) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
  @Suppress("DEPRECATION")
  private fun installed() = context.packageManager.getPackageInfo(context.packageName, flags)
  @Suppress("DEPRECATION")
  private fun signatures(info: PackageInfo): Set<String> {
    val signatures = if (Build.VERSION.SDK_INT >= 28) info.signingInfo?.apkContentsSigners else info.signatures
    return signatures?.map { digest(it.toByteArray()) }?.toSet() ?: emptySet()
  }
  private fun digest(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
  private fun fileHash(file: File): String {
    val hash = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { stream ->
      val buffer = ByteArray(65536)
      while (true) { val n = stream.read(buffer); if (n < 0) break; hash.update(buffer, 0, n) }
    }
    return hash.digest().joinToString("") { "%02x".format(it) }
  }
  private fun validate(file: File, hash: String, version: String, versionCode: Long) {
    require(fileHash(file).equals(hash, true)) { "La descarga no está completa. Vuelve a intentarlo." }
    val candidate = requireNotNull(context.packageManager.getPackageArchiveInfo(file.path, flags)) { "El archivo no es una aplicación válida." }
    val current = installed()
    require(candidate.packageName == "com.a1var0w.safeeat" && candidate.packageName == context.packageName) { "El archivo no corresponde a SafeEat." }
    require(candidate.versionName == version && code(candidate) == versionCode && versionCode > code(current)) { "La versión descargada no corresponde a esta actualización." }
    require(signatures(candidate).isNotEmpty() && signatures(candidate) == signatures(current)) { "La firma de la actualización no coincide con SafeEat." }
  }
  private fun allowed(url: URL) = url.protocol == "https" && url.host == "chat-ink.tail552c89.ts.net" && url.port == 10000 && url.userInfo == null && url.query == null && url.ref == null && Regex("/safeeat-downloads/releases/SafeEat-[0-9]+\\.[0-9]+\\.[0-9]+\\.apk").matches(url.path)
  private fun canInstall() = Build.VERSION.SDK_INT < 26 || context.packageManager.canRequestPackageInstalls()

  override fun definition() = ModuleDefinition {
    Name("SafeEatUpdater")
    Events("downloadProgress")
    OnCreate { directory.listFiles()?.forEach { it.delete() } }
    Function("installedVersion") { val info = installed(); mapOf("version" to info.versionName, "versionCode" to code(info).toDouble()) }
    Function("canInstall") { canInstall() }
    Function("cancelDownload") { cancelled.set(true); connection?.disconnect() }
    AsyncFunction("download") { address: String, hash: String, size: Double, version: String, versionCode: Double, promise: Promise ->
      if (!busy.compareAndSet(false, true)) { promise.reject("BUSY", "Ya hay una descarga en curso.", null) }
      else {
        cancelled.set(false)
        verified = null
        worker.execute {
          val part = File(directory, "SafeEat.part")
          try {
            val url = URL(address)
            require(allowed(url) && Regex("[a-fA-F0-9]{64}").matches(hash) && size > 100000 && size <= 262144000 && size == size.toLong().toDouble()) { "No se puede verificar esta actualización." }
            apk.delete(); part.delete()
            require(directory.usableSpace > size.toLong() * 2) { "Libera espacio en el móvil antes de actualizar." }
            val conn = url.openConnection() as HttpsURLConnection
            connection = conn
            conn.connectTimeout = 15000; conn.readTimeout = 20000; conn.instanceFollowRedirects = false
            require(conn.responseCode == 200) { "No se ha podido descargar la actualización." }
            require(conn.contentLengthLong == size.toLong()) { "El tamaño de la actualización no coincide." }
            var total = 0L; var lastEvent = 0L
            conn.inputStream.use { input -> part.outputStream().use { output ->
              val buffer = ByteArray(65536)
              while (true) {
                if (cancelled.get()) throw InterruptedException()
                val n = input.read(buffer); if (n < 0) break
                total += n; require(total <= size.toLong()) { "El archivo supera el tamaño esperado." }; output.write(buffer, 0, n)
                if (System.currentTimeMillis() - lastEvent > 250) {
                  sendEvent("downloadProgress", mapOf("received" to total.toDouble(), "total" to size))
                  lastEvent = System.currentTimeMillis()
                }
              }
            } }
            if (cancelled.get()) throw InterruptedException()
            require(total == size.toLong()) { "La descarga se ha interrumpido. Vuelve a intentarlo." }
            validate(part, hash, version, versionCode.toLong())
            if (cancelled.get()) throw InterruptedException()
            require(part.renameTo(apk)) { "No se ha podido guardar la actualización." }
            verified = Triple(hash, version, versionCode.toLong())
            sendEvent("downloadProgress", mapOf("received" to size, "total" to size))
            promise.resolve(null)
          } catch (error: Exception) {
            part.delete(); apk.delete()
            promise.reject(if (cancelled.get()) "CANCELLED" else "DOWNLOAD_FAILED", if (cancelled.get()) "Descarga cancelada." else if (error is java.io.IOException) "La descarga se ha interrumpido. Comprueba tu conexión y vuelve a intentarlo." else error.message ?: "No se ha podido descargar. Vuelve a intentarlo.", error)
          } finally { connection?.disconnect(); connection = null; busy.set(false) }
        }
      }
    }
    AsyncFunction("openInstallSettings") {
      if (Build.VERSION.SDK_INT >= 26) {
        val activity = requireNotNull(appContext.currentActivity)
        activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}")))
      }
    }.runOnQueue(Queues.MAIN)
    AsyncFunction("install") { promise: Promise ->
      worker.execute {
        try {
          check(canInstall()) { "Permite instalar aplicaciones desde SafeEat para continuar." }
          val ready = requireNotNull(verified) { "Descarga la actualización de nuevo." }
          validate(apk, ready.first, ready.second, ready.third)
          val activity = requireNotNull(appContext.currentActivity)
          val uri = FileProvider.getUriForFile(context, "${context.packageName}.updates", apk)
          activity.runOnUiThread {
            try {
              val intent = Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
              intent.clipData = ClipData.newRawUri("SafeEat", uri)
              activity.startActivity(intent)
              promise.resolve(null)
            } catch (error: Exception) { promise.reject("INSTALL_FAILED", "No se ha podido abrir el instalador.", error) }
          }
        } catch (error: Exception) { promise.reject("INSTALL_FAILED", error.message, error) }
      }
    }
    OnDestroy { cancelled.set(true); connection?.disconnect(); worker.shutdown() }
  }
}
