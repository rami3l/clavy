// swift-tools-version: 6.0
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
  name: "Claveilleur",
  platforms: [
    .macOS(.v15)
  ],
  products: [
    .executable(name: "claveilleur", targets: ["Claveilleur"])
  ],
  dependencies: [
    .package(
      url: "https://github.com/apple/swift-log.git",
      from: "1.5.0"
    ),
    .package(
      url: "https://github.com/apple/swift-argument-parser",
      from: "1.2.0"
    ),
    .package(
      url: "https://github.com/emorydunn/LaunchAgent",
      from: "0.3.0"
    ),
  ],
  targets: [
    // Targets are the basic building blocks of a package, defining a module or a test suite.
    // Targets can depend on other targets in this package and products from dependencies.
    .executableTarget(
      name: "Claveilleur",
      dependencies: [
        .product(name: "ArgumentParser", package: "swift-argument-parser"),
        .product(name: "Logging", package: "swift-log"),
        .product(name: "LaunchAgent", package: "LaunchAgent"),
      ],
      path: "Sources",
      // https://forums.swift.org/t/swift-package-manager-use-of-info-plist-use-for-apps/6532/13
      linkerSettings: [
        .unsafeFlags([
          "-Xlinker", "-sectcreate",
          "-Xlinker", "__TEXT",
          "-Xlinker", "__info_plist",
          "-Xlinker", "Supporting/Info.plist",
        ])
      ]
    )
  ]
)
