// swift-tools-version: 6.0
// dictate-ane: Parakeet on the Neural Engine via FluidAudio (Core ML), as a helper process for
// dictate's "ane" engine (core/src/engine.rs). Built by scripts/bundle.sh into Contents/MacOS.
import PackageDescription

let package = Package(
    name: "dictate-ane",
    platforms: [.macOS(.v14)],
    dependencies: [
        // main as of 2026-10-31: Parakeet Ultra (`AsrModelVersion.ultra`) is newer than the last tag (v0.9.1).
        .package(url: "https://github.com/FluidInference/FluidAudio.git", revision: "6932c6fd2628ba11bc597c9a3a519ac99615d4a2")
    ],
    targets: [
        .executableTarget(
            name: "dictate-ane",
            dependencies: [.product(name: "FluidAudio", package: "FluidAudio")],
            path: "Sources/dictate-ane"
        )
    ]
)
