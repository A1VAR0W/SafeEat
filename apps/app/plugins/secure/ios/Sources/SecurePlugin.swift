import Foundation
import Security
import SwiftRs
import Tauri
import UIKit
import WebKit

class KeyArgs: Decodable {
  let key: String
}

class SetArgs: Decodable {
  let key: String
  let value: String
}

/// Small secret store on the iOS Keychain. Items never leave this device and are
/// readable only while it is unlocked.
class SecurePlugin: Plugin {
  private let service = "com.a1var0w.safeeat.session"

  private func query(_ key: String) -> [String: Any] {
    return [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: key,
    ]
  }

  @objc public func set(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(SetArgs.self)
    SecItemDelete(query(args.key) as CFDictionary)
    var item = query(args.key)
    item[kSecValueData as String] = Data(args.value.utf8)
    item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
    let status = SecItemAdd(item as CFDictionary, nil)
    if status == errSecSuccess {
      invoke.resolve()
    } else {
      invoke.reject("No se pudo guardar de forma segura")
    }
  }

  @objc public func get(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(KeyArgs.self)
    var search = query(args.key)
    search[kSecReturnData as String] = true
    search[kSecMatchLimit as String] = kSecMatchLimitOne
    var found: CFTypeRef?
    let status = SecItemCopyMatching(search as CFDictionary, &found)
    if status == errSecSuccess, let data = found as? Data,
      let value = String(data: data, encoding: .utf8)
    {
      invoke.resolve(["value": value])
    } else {
      invoke.resolve(["value": NSNull()])
    }
  }

  @objc public func remove(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(KeyArgs.self)
    SecItemDelete(query(args.key) as CFDictionary)
    invoke.resolve()
  }
}

@_cdecl("init_plugin_safeeat_secure")
func initPlugin() -> Plugin {
  return SecurePlugin()
}
