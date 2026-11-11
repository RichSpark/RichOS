// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "RichFiles",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "RichFilesCore", targets: ["RichFilesCore"]),
        .library(name: "RichFilesBrowser", targets: ["RichFiles"]),
        .library(name: "RichFilesRail", type: .dynamic, targets: ["RichFilesRail"]),
        .executable(name: "RichFiles", targets: ["RichFilesApp"])
    ],
    targets: [
        .target(name: "RichFilesCore"),
        .target(name: "RichFiles", dependencies: ["RichFilesCore"]),
        .executableTarget(name: "RichFilesApp", dependencies: ["RichFiles"]),
        .target(name: "RichFilesRail", dependencies: ["RichFiles"]),
        .testTarget(name: "RichFilesCoreTests", dependencies: ["RichFilesCore"]),
        .testTarget(name: "RichFilesTests", dependencies: ["RichFiles", "RichFilesCore"])
    ]
)
