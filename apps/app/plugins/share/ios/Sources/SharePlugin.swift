import SwiftRs
import Tauri
import UIKit
import WebKit

class ShareArgs: Decodable {
  let text: String
  let title: String?
}

/// Opens the iOS share sheet with a plain-text message (a SafeEat link).
class SharePlugin: Plugin {
  @objc public func share(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ShareArgs.self)
    DispatchQueue.main.async {
      guard let root = self.manager.viewController else {
        invoke.reject("No se pudo abrir el menú de compartir")
        return
      }
      let sheet = UIActivityViewController(activityItems: [args.text], applicationActivities: nil)
      // iPad presents the sheet as a popover anchored to the view.
      sheet.popoverPresentationController?.sourceView = root.view
      sheet.popoverPresentationController?.sourceRect = CGRect(
        x: root.view.bounds.midX, y: root.view.bounds.maxY - 80, width: 0, height: 0)
      root.present(sheet, animated: true)
      invoke.resolve()
    }
  }
}

@_cdecl("init_plugin_safeeat_share")
func initPlugin() -> Plugin {
  return SharePlugin()
}
