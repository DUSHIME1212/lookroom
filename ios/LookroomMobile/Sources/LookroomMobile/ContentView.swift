import SwiftUI
import UniformTypeIdentifiers
import AVKit
import CoreImage

private let accent = Color(red: 0.52, green: 0.70, blue: 0.79)

struct ContentView: View {
    @State private var projects: [MobileProject] = []
    @State private var selected: MobileProject?
    @State private var importing = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    HStack(spacing: 12) {
                        Image("Icon", bundle: .module).resizable().frame(width: 48, height: 48)
                        VStack(alignment: .leading) {
                            Text("LOOKROOM").font(.headline).foregroundStyle(accent)
                            Text("Your next story starts here.").font(.title2.bold())
                        }
                    }
                    Button { importing = true } label: {
                        Label("Import photo or video", systemImage: "plus").frame(maxWidth: .infinity).padding(.vertical, 8)
                    }.buttonStyle(.borderedProminent).tint(Color(red: 0.32, green: 0.47, blue: 0.55))
                    Text("Recent projects").font(.headline)
                    if projects.isEmpty {
                        Text("Import media to start. Your projects stay on this iPhone.").foregroundStyle(.secondary)
                    }
                    ForEach(projects) { project in
                        Button { selected = project } label: {
                            HStack {
                                Image(systemName: project.isVideo ? "film" : "photo").frame(width: 28)
                                VStack(alignment: .leading, spacing: 5) {
                                    Text(project.name).foregroundStyle(.primary).lineLimit(1)
                                    Text(project.modified, style: .date).font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Image(systemName: "chevron.right").foregroundStyle(.secondary)
                            }.padding(12).background(.white.opacity(0.05), in: RoundedRectangle(cornerRadius: 4))
                        }.buttonStyle(.plain)
                    }
                }.padding(20)
            }
            .navigationTitle("Lookroom").navigationBarTitleDisplayMode(.inline)
            .fileImporter(isPresented: $importing, allowedContentTypes: [.image, .movie]) { result in
                do {
                    let source = try result.get()
                    let access = source.startAccessingSecurityScopedResource()
                    defer { if access { source.stopAccessingSecurityScopedResource() } }
                    try MobileFiles.prepare()
                    let name = "\(UUID().uuidString).\(source.pathExtension)"
                    try FileManager.default.copyItem(at: source, to: MobileFiles.media.appendingPathComponent(name))
                    let type = try source.resourceValues(forKeys: [.contentTypeKey]).contentType ?? UTType(filenameExtension: source.pathExtension)
                    let project = MobileProject(name: source.deletingPathExtension().lastPathComponent, mediaFile: name, isVideo: type?.conforms(to: .movie) == true)
                    try MobileFiles.save(project)
                    reload()
                    selected = project
                } catch { self.error = error.localizedDescription }
            }
            .sheet(item: $selected, onDismiss: reload) { project in MobileEditor(project: project) }
            .alert("Lookroom", isPresented: Binding(get: { error != nil }, set: { if !$0 { error = nil } })) {
                Button("OK") { error = nil }
            } message: { Text(error ?? "") }
            .onAppear(perform: reload)
        }.tint(accent).preferredColorScheme(.dark).buttonBorderShape(.roundedRectangle(radius: 4))
    }

    private func reload() {
        do { projects = try MobileFiles.load() } catch { self.error = error.localizedDescription }
    }
}

@MainActor
final class MobileEditorModel: ObservableObject {
    @Published var project: MobileProject
    @Published var preview: UIImage?
    @Published var error: String?
    @Published var exporting = false
    @Published var exported: URL?
    let player = AVPlayer()
    private var revision = UUID()
    private var pending: DispatchWorkItem?
    private var session: AVAssetExportSession?

    init(project: MobileProject) {
        self.project = project
        if project.isVideo {
            player.replaceCurrentItem(with: AVPlayerItem(url: MobileFiles.media.appendingPathComponent(project.mediaFile)))
        }
        refresh()
    }

    func save() {
        project.modified = Date()
        do { try MobileFiles.save(project) } catch { self.error = error.localizedDescription }
    }

    func refresh() {
        pending?.cancel()
        let current = UUID()
        revision = current
        let snapshot = project
        let work = DispatchWorkItem { [weak self] in
            guard let self = self, self.revision == current else { return }
            if snapshot.isVideo {
                if let item = self.player.currentItem { item.videoComposition = Self.composition(item.asset, grade: snapshot.grade) }
            } else {
                DispatchQueue.global(qos: .userInitiated).async {
                    let result = Result { try MobileFiles.picture(snapshot, preview: true) }
                    DispatchQueue.main.async {
                        guard self.revision == current else { return }
                        switch result {
                        case .success(let image): self.preview = image
                        case .failure(let error): self.error = error.localizedDescription
                        }
                    }
                }
            }
        }
        pending = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08, execute: work)
    }

    private static func composition(_ asset: AVAsset, grade: MobileGrade) -> AVVideoComposition {
        AVVideoComposition(asset: asset, applyingCIFiltersWithHandler: { request in
            let source = request.sourceImage
            let image = grade.apply(to: source.clampedToExtent()).cropped(to: source.extent)
            request.finish(with: image, context: MobileFiles.context)
        })
    }

    func export() {
        guard !exporting else { return }
        save()
        exported = nil
        exporting = true
        let snapshot = project
        do { try MobileFiles.prepare() } catch { self.error = error.localizedDescription; exporting = false; return }
        if project.isVideo {
            let asset = AVURLAsset(url: MobileFiles.media.appendingPathComponent(project.mediaFile))
            guard let job = AVAssetExportSession(asset: asset, presetName: AVAssetExportPresetHighestQuality) else {
                error = "This video cannot be exported on this iPhone."; exporting = false; return
            }
            let type: AVFileType = job.supportedFileTypes.contains(.mp4) ? .mp4 : .mov
            let url = MobileFiles.exports.appendingPathComponent("Lookroom-\(UUID().uuidString).\(type == .mp4 ? "mp4" : "mov")")
            job.outputURL = url
            job.outputFileType = type
            job.videoComposition = Self.composition(asset, grade: snapshot.grade)
            job.shouldOptimizeForNetworkUse = true
            session = job
            job.exportAsynchronously { [weak self] in
                DispatchQueue.main.async {
                    guard let self = self, let finished = self.session else { return }
                    self.exporting = false
                    self.session = nil
                    if finished.status == .completed { self.exported = url }
                    else {
                        try? FileManager.default.removeItem(at: url)
                        self.error = finished.status == .cancelled ? "Export cancelled." : (finished.error?.localizedDescription ?? "Export failed.")
                    }
                }
            }
        } else {
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                let result = Result<URL, Error> {
                    let image = try MobileFiles.picture(snapshot, preview: false)
                    guard let data = image.jpegData(compressionQuality: 0.95) else { throw CocoaError(.fileWriteUnknown) }
                    let url = MobileFiles.exports.appendingPathComponent("Lookroom-\(UUID().uuidString).jpg")
                    try data.write(to: url, options: .atomic)
                    return url
                }
                DispatchQueue.main.async {
                    guard let self = self else { return }
                    self.exporting = false
                    switch result {
                    case .success(let url): self.exported = url
                    case .failure(let error): self.error = error.localizedDescription
                    }
                }
            }
        }
    }

    func close() { pending?.cancel(); player.pause(); save() }
    func cancelExport() { session?.cancelExport() }
}

struct MobileEditor: View {
    @Environment(\.dismiss) private var dismiss
    @StateObject private var model: MobileEditorModel

    init(project: MobileProject) { _model = StateObject(wrappedValue: MobileEditorModel(project: project)) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 18) {
                    if model.project.isVideo {
                        NativeVideoPlayer(player: model.player).frame(height: 260)
                    } else if let image = model.preview {
                        Image(uiImage: image).resizable().scaledToFit().frame(maxHeight: 350)
                    } else { ProgressView().frame(height: 200) }
                    VStack(spacing: 18) {
                        HStack { Text("Adjustments").font(.headline); Spacer(); Button("Reset all") { model.project.grade = MobileGrade() } }
                        Adjustment(label: "Exposure", value: $model.project.grade.exposure, range: -3...3, neutral: 0)
                        Adjustment(label: "Contrast", value: $model.project.grade.contrast, range: 0.5...2, neutral: 1)
                        Adjustment(label: "Saturation", value: $model.project.grade.saturation, range: 0...2, neutral: 1)
                        Adjustment(label: "Warmth", value: $model.project.grade.warmth, range: -2500...2500, neutral: 0)
                        Adjustment(label: "Sharpen", value: $model.project.grade.sharpen, range: 0...2, neutral: 0)
                        Adjustment(label: "Vignette", value: $model.project.grade.vignette, range: 0...2, neutral: 0)
                    }.padding(16).background(.white.opacity(0.04), in: RoundedRectangle(cornerRadius: 4))
                    if model.exporting {
                        ProgressView("Exporting…")
                        if model.project.isVideo { Button("Cancel export") { model.cancelExport() } }
                    } else { Button("Export graded media") { model.export() }.buttonStyle(.borderedProminent) }
                    if let url = model.exported { ShareLink(item: url) { Label("Share or save export", systemImage: "square.and.arrow.up") } }
                }.padding(16)
            }.scrollDismissesKeyboard(.interactively)
            .navigationTitle(model.project.name).navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .topBarLeading) { Button("Done") { dismiss() }.disabled(model.exporting) } }
            .interactiveDismissDisabled(model.exporting)
            .onChange(of: model.project.grade) { _, _ in model.refresh(); model.save() }
            .onDisappear { model.close() }
            .alert("Lookroom", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
                Button("OK") { model.error = nil }
            } message: { Text(model.error ?? "") }
        }.tint(accent).preferredColorScheme(.dark).buttonBorderShape(.roundedRectangle(radius: 4))
    }
}

private struct Adjustment: View {
    let label: String
    @Binding var value: Double
    let range: ClosedRange<Double>
    let neutral: Double

    var body: some View {
        VStack(spacing: 8) {
            HStack {
                Text(label).foregroundStyle(.secondary)
                Spacer()
                TextField(label, value: $value, format: .number.precision(.fractionLength(0...2)))
                    .keyboardType(.numbersAndPunctuation).multilineTextAlignment(.trailing).frame(width: 82)
                    .padding(6).background(.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 4))
                    .onChange(of: value) { _, new in value = new.isFinite ? min(range.upperBound, max(range.lowerBound, new)) : neutral }
                Button { value = neutral } label: { Image(systemName: "arrow.counterclockwise") }.accessibilityLabel("Reset \(label)")
            }.font(.subheadline)
            GeometryReader { geometry in
                let width = max(1, geometry.size.width - 12)
                let fraction = (value - range.lowerBound) / (range.upperBound - range.lowerBound)
                ZStack(alignment: .leading) {
                    Rectangle().fill(.white.opacity(0.18)).frame(height: 4)
                    Rectangle().fill(accent).frame(width: width * CGFloat(fraction) + 6, height: 4)
                    RoundedRectangle(cornerRadius: 2).fill(.white).frame(width: 10, height: 22).offset(x: width * CGFloat(fraction))
                }.frame(height: 32).contentShape(Rectangle())
                    .gesture(DragGesture(minimumDistance: 0).onChanged { gesture in
                        value = range.lowerBound + min(1, max(0, Double((gesture.location.x - 6) / width))) * (range.upperBound - range.lowerBound)
                    })
                    .accessibilityElement().accessibilityLabel(label).accessibilityValue(String(format: "%.2f", value))
                    .accessibilityAdjustableAction { direction in
                        let step = (range.upperBound - range.lowerBound) / 200
                        switch direction {
                        case .increment: value = min(range.upperBound, value + step)
                        case .decrement: value = max(range.lowerBound, value - step)
                        @unknown default: break
                        }
                    }
            }.frame(height: 32)
        }
    }
}

// AVKit's UIKit controller works with xtool even when the SwiftUI VideoPlayer
// overlay is unavailable in the extracted SDK.
private struct NativeVideoPlayer: UIViewControllerRepresentable {
    let player: AVPlayer

    func makeUIViewController(context: Context) -> AVPlayerViewController {
        let controller = AVPlayerViewController()
        controller.player = player
        controller.showsPlaybackControls = true
        return controller
    }

    func updateUIViewController(_ controller: AVPlayerViewController, context: Context) {
        if controller.player !== player { controller.player = player }
    }
}
