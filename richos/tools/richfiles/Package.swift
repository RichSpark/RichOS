// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "RichFiles",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "RichFilesCore", targets: ["RichFilesCore"]),
        .executable(name: "RichFiles", targets: ["RichFiles"])
    ],
    targets: [
        .target(name: "RichFilesCore"),
        .executableTarget(name: "RichFiles", dependencies: ["RichFilesCore"]),
        .testTarget(name: "RichFilesCoreTests", dependencies: ["RichFilesCore"]),
        .testTarget(name: "RichFilesTests", dependencies: ["RichFiles", "RichFilesCore"])
    ]
)
