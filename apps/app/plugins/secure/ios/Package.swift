// swift-tools-version:5.3
import PackageDescription

let package = Package(
  name: "tauri-plugin-safeeat-secure",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v13),
  ],
  products: [
    .library(
      name: "tauri-plugin-safeeat-secure",
      type: .static,
      targets: ["tauri-plugin-safeeat-secure"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-safeeat-secure",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
