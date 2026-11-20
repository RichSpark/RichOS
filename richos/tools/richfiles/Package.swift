// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "RichFiles",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "RichFilesCore", targets: ["RichFilesCore"]),
        .library(name: "RichFilesBrowser", targets: ["RichFiles"]),
        .library(name: "RichFilesIntegration", type: .dynamic, targets: ["RichFilesIntegration"]),
        .executable(name: "RichFiles", targets: ["RichFilesApp"]),
        .executable(name: "RichGlance", targets: ["RichGlance"]),
        .executable(name: "RichFilesWindowGestures", targets: ["RichFilesWindowGestures"])
    ],
    targets: [
        .target(name: "RichFilesCore"),
        .target(name: "RichFiles", dependencies: ["RichFilesCore"]),
        .executableTarget(name: "RichFilesApp", dependencies: ["RichFiles"]),
        .executableTarget(name: "RichGlance", dependencies: ["RichFilesCore"]),
        .executableTarget(name: "RichFilesWindowGestures", dependencies: ["RichFilesCore"]),
        .target(name: "RichFilesIntegration", dependencies: ["RichFiles", "RichFilesCore"]),
        .testTarget(name: "RichFilesCoreTests", dependencies: ["RichFilesCore"], resources: [.copy("Fixtures/Glance")]),
        .testTarget(name: "RichFilesTests", dependencies: ["RichFiles", "RichFilesCore"]),
        .testTarget(name: "RichGlanceTests", dependencies: ["RichGlance"])
    ]
)
