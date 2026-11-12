package com.a1var0w.safeeat.share

import android.app.Activity
import android.content.Intent
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class ShareArgs {
  lateinit var text: String
  var title: String = ""
}

/** Opens the system chooser to share a plain-text message (a SafeEat link). */
@TauriPlugin
class SharePlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun share(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(ShareArgs::class.java)
      val send = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_TEXT, args.text)
        if (args.title.isNotEmpty()) putExtra(Intent.EXTRA_SUBJECT, args.title)
      }
      activity.runOnUiThread {
        activity.startActivity(Intent.createChooser(send, args.title.ifEmpty { null }))
      }
      invoke.resolve()
    } catch (error: Exception) {
      invoke.reject("No se pudo abrir el menú de compartir")
    }
  }
}
