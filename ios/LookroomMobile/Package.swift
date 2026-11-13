// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "LookroomMobile",
    platforms: [
        .iOS(.v17),
    ],
    products: [
        // An xtool project should contain exactly one library product,
        // representing the main app.
        .library(
            name: "LookroomMobile",
            targets: ["LookroomMobile"]
        ),
    ],
    targets: [
        .target(
            name: "LookroomMobile",
            resources: [.process("Resources")]
        ),
    ],
    swiftLanguageModes: [.v5]
)
