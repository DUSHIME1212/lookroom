import Foundation
import CoreImage
import UIKit

struct MobileGrade: Codable, Equatable {
    var exposure = 0.0
    var contrast = 1.0
    var saturation = 1.0
    var warmth = 0.0
    var sharpen = 0.0
    var vignette = 0.0

    func apply(to image: CIImage) -> CIImage {
        image.applyingFilter("CIExposureAdjust", parameters: [kCIInputEVKey: exposure])
            .applyingFilter("CIColorControls", parameters: [kCIInputContrastKey: contrast, kCIInputSaturationKey: saturation])
            .applyingFilter("CITemperatureAndTint", parameters: ["inputNeutral": CIVector(x: 6500, y: 0), "inputTargetNeutral": CIVector(x: CGFloat(6500 + warmth), y: 0)])
            .applyingFilter("CISharpenLuminance", parameters: [kCIInputSharpnessKey: sharpen])
            .applyingFilter("CIVignette", parameters: [kCIInputIntensityKey: vignette, kCIInputRadiusKey: 1.0])
    }
}

struct MobileProject: Codable, Identifiable {
    var id = UUID()
    var name: String
    var mediaFile: String
    var isVideo: Bool
    var modified = Date()
    var grade = MobileGrade()
}

enum MobileFiles {
    static let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("Lookroom", isDirectory: true)
    static var media: URL { root.appendingPathComponent("Media", isDirectory: true) }
    static var projects: URL { root.appendingPathComponent("Projects", isDirectory: true) }
    static var exports: URL { root.appendingPathComponent("Exports", isDirectory: true) }
    static let context = CIContext()

    static func prepare() throws {
        for directory in [media, projects, exports] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
    }

    static func save(_ project: MobileProject) throws {
        try prepare()
        let data = try JSONEncoder().encode(project)
        try data.write(to: projects.appendingPathComponent("\(project.id.uuidString).json"), options: .atomic)
    }

    static func load() throws -> [MobileProject] {
        try prepare()
        return try FileManager.default.contentsOfDirectory(at: projects, includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "json" }
            .map { try JSONDecoder().decode(MobileProject.self, from: Data(contentsOf: $0)) }
            .sorted { $0.modified > $1.modified }
    }

    static func picture(_ project: MobileProject, preview: Bool) throws -> UIImage {
        let url = media.appendingPathComponent(project.mediaFile)
        guard var source = CIImage(contentsOf: url, options: [.applyOrientationProperty: true]) else {
            throw NSError(domain: "Lookroom", code: 1, userInfo: [NSLocalizedDescriptionKey: "This picture could not be opened."])
        }
        if preview {
            let scale = min(1, 1400 / max(source.extent.width, source.extent.height))
            source = source.transformed(by: CGAffineTransform(scaleX: scale, y: scale))
        }
        let result = project.grade.apply(to: source).cropped(to: source.extent)
        guard let image = context.createCGImage(result, from: result.extent) else {
            throw NSError(domain: "Lookroom", code: 2, userInfo: [NSLocalizedDescriptionKey: "This picture could not be rendered."])
        }
        return UIImage(cgImage: image)
    }
}
