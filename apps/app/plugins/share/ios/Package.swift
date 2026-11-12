// swift-tools-version:5.3
import PackageDescription

let package = Package(
  name: "tauri-plugin-safeeat-share",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v13),
  ],
  products: [
    .library(
      name: "tauri-plugin-safeeat-share",
      type: .static,
      targets: ["tauri-plugin-safeeat-share"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-safeeat-share",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
