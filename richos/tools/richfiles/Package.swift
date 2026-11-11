// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "RichFiles",
    platforms: [.macOS(.v13)],
    products: [.library(name: "RichFilesCore", targets: ["RichFilesCore"])],
    targets: [
        .target(name: "RichFilesCore"),
        .testTarget(name: "RichFilesCoreTests", dependencies: ["RichFilesCore"])
    ]
)
