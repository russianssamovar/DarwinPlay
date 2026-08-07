// swift-tools-version: 6.2
import PackageDescription

var products: [Product] = [
  .library(name: "DarwinPlayCore", targets: ["DarwinPlayCore"])
]

var targets: [Target] = [
  .target(name: "DarwinPlayCore"),
  .testTarget(name: "DarwinPlayCoreTests", dependencies: ["DarwinPlayCore"]),
]

#if os(macOS)
  products.append(.executable(name: "DarwinPlay", targets: ["DarwinPlay"]))
  targets.append(.executableTarget(name: "DarwinPlay", dependencies: ["DarwinPlayCore"]))
#endif

let package = Package(
  name: "DarwinPlay",
  platforms: [.macOS(.v15)],
  products: products,
  targets: targets
)
