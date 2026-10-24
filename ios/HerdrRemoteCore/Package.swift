// swift-tools-version: 5.9
// The companion's logic that needs no UIKit, ActivityKit, or Keychain:
// pairing links, fingerprints, base64url, session signatures, the Live
// Activity content, and the API's JSON. It builds for macOS too, so
// `swift test` runs it without a simulator.

import PackageDescription

let package = Package(
    name: "HerdrRemoteCore",
    platforms: [.iOS("17.2"), .macOS(.v14)],
    products: [
        .library(name: "HerdrRemoteCore", targets: ["HerdrRemoteCore"]),
    ],
    targets: [
        .target(name: "HerdrRemoteCore"),
        .testTarget(
            name: "HerdrRemoteCoreTests",
            dependencies: ["HerdrRemoteCore"],
            resources: [.copy("Fixtures")]
        ),
    ]
)
