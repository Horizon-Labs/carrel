// Drawing a handwritten note on a card (FORMAT.md §5.10): PencilKit's canvas and tool picker
// on iPad, for Apple Pencil, and a pointer canvas on a Mac, which has no PencilKit canvas. On
// saving, the drawing is drawn as a PNG on white, and Vision reads its handwriting on this
// device, so search, the librarian and every interface find the note by its words.

import CarrelKit
import ImageIO
import PencilKit
import SwiftUI
import UniformTypeIdentifiers
import Vision

/// A drawing ready to save: its PencilKit data, a PNG of it, and its recognised text.
struct DrawnNote: Sendable {
    let data: Data
    let picture: Data
    let text: String
}

struct DrawingEditor: View {
    /// The drawing it changes; `nil` for a new one.
    let existing: CardDrawingInfo?
    let save: (DrawnNote) async throws -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var drawing = PKDrawing()
    @State private var saving = false
    @State private var problem: String?

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                canvas
                if let problem {
                    Text(problem).font(.callout).foregroundStyle(.red).padding(8)
                }
            }
            .navigationTitle(existing == nil ? "New Note" : "Note")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if saving {
                        ProgressView()
                    } else {
                        Button("Save") { Task { await finish() } }
                            .disabled(drawing.strokes.isEmpty)
                    }
                }
                #if os(macOS)
                    ToolbarItem {
                        Button("Undo Stroke", systemImage: "arrow.uturn.backward") {
                            _ = drawing.strokes.popLast()
                        }
                        .disabled(drawing.strokes.isEmpty)
                    }
                #endif
            }
        }
        #if os(macOS)
            .frame(minWidth: 640, minHeight: 480)
        #endif
        .task {
            guard let existing,
                let data = try? Data(contentsOf: URL(filePath: existing.dataPath)),
                let saved = try? PKDrawing(data: data)
            else { return }
            drawing = saved
        }
    }

    @ViewBuilder private var canvas: some View {
        #if os(iOS)
            PencilCanvas(drawing: $drawing)
                .accessibilityLabel("Drawing")
        #else
            PointerCanvas(drawing: $drawing)
                .accessibilityLabel("Drawing")
        #endif
    }

    private func finish() async {
        saving = true
        defer { saving = false }
        guard let picture = whitePicture(of: drawing), let png = pngData(picture) else {
            problem = "The drawing couldn’t be drawn as a picture."
            return
        }
        let text = await recognisedText(in: picture)
        do {
            try await save(DrawnNote(data: drawing.dataRepresentation(), picture: png, text: text))
            dismiss()
        } catch {
            problem = (error as? CarrelError)?.message ?? error.localizedDescription
        }
    }
}

/// The scale drawings are pictured at.
private let pictureScale: CGFloat = 2

/// `drawing` within `rect`, in light colours whatever the appearance, so black ink stays
/// black.
@MainActor
func lightPicture(of drawing: PKDrawing, in rect: CGRect) -> CGImage? {
    #if os(iOS)
        var image: UIImage?
        UITraitCollection(userInterfaceStyle: .light).performAsCurrent {
            image = drawing.image(from: rect, scale: pictureScale)
        }
        return image?.cgImage
    #else
        var image: NSImage?
        (NSAppearance(named: .aqua) ?? NSAppearance.currentDrawing())
            .performAsCurrentDrawingAppearance {
                image = drawing.image(from: rect, scale: pictureScale)
            }
        return image?.cgImage(forProposedRect: nil, context: nil, hints: nil)
    #endif
}

/// `drawing`, with a margin around its strokes, on white, as a card shows it in any reader.
@MainActor
private func whitePicture(of drawing: PKDrawing) -> CGImage? {
    let bounds = drawing.bounds.insetBy(dx: -24, dy: -24)
    guard !drawing.strokes.isEmpty, let ink = lightPicture(of: drawing, in: bounds),
        let context = CGContext(
            data: nil, width: ink.width, height: ink.height, bitsPerComponent: 8,
            bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { return nil }
    let all = CGRect(x: 0, y: 0, width: ink.width, height: ink.height)
    context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
    context.fill(all)
    context.draw(ink, in: all)
    return context.makeImage()
}

private func pngData(_ image: CGImage) -> Data? {
    let data = NSMutableData()
    guard
        let destination = CGImageDestinationCreateWithData(
            data as CFMutableData, UTType.png.identifier as CFString, 1, nil)
    else { return nil }
    CGImageDestinationAddImage(destination, image, nil)
    return CGImageDestinationFinalize(destination) ? data as Data : nil
}

/// The handwriting Vision reads in `picture`, line by line, top first; empty if none.
private func recognisedText(in picture: CGImage) async -> String {
    await Task.detached(priority: .userInitiated) {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        let handler = VNImageRequestHandler(cgImage: picture)
        guard (try? handler.perform([request])) != nil, let lines = request.results else {
            return ""
        }
        return
            lines
            .sorted { $0.boundingBox.maxY > $1.boundingBox.maxY }
            .compactMap { $0.topCandidates(1).first?.string }
            .joined(separator: "\n")
    }.value
}

#if os(iOS)
    /// PencilKit's canvas, with the tool picker, on white.
    private struct PencilCanvas: UIViewRepresentable {
        @Binding var drawing: PKDrawing

        func makeCoordinator() -> Coordinator { Coordinator(drawing: $drawing) }

        func makeUIView(context: Context) -> PKCanvasView {
            let canvas = PKCanvasView()
            canvas.drawing = drawing
            #if targetEnvironment(simulator)
                canvas.drawingPolicy = .anyInput
            #else
                canvas.drawingPolicy = .default
            #endif
            canvas.tool = PKInkingTool(.pen, color: .black, width: 4)
            canvas.backgroundColor = .white
            canvas.isOpaque = true
            canvas.overrideUserInterfaceStyle = .light
            canvas.alwaysBounceVertical = true
            canvas.delegate = context.coordinator
            let picker = context.coordinator.picker
            picker.overrideUserInterfaceStyle = .light
            picker.setVisible(true, forFirstResponder: canvas)
            picker.addObserver(canvas)
            Task { @MainActor in canvas.becomeFirstResponder() }
            return canvas
        }

        func updateUIView(_ canvas: PKCanvasView, context: Context) {
            if canvas.drawing != drawing { canvas.drawing = drawing }
        }

        @MainActor
        final class Coordinator: NSObject, PKCanvasViewDelegate {
            let drawing: Binding<PKDrawing>
            let picker = PKToolPicker()

            init(drawing: Binding<PKDrawing>) { self.drawing = drawing }

            func canvasViewDrawingDidChange(_ canvas: PKCanvasView) {
                drawing.wrappedValue = canvas.drawing
            }
        }
    }
#else
    /// A canvas drawn on with the pointer or trackpad, adding PencilKit strokes, since a Mac
    /// has no PencilKit canvas.
    private struct PointerCanvas: View {
        @Binding var drawing: PKDrawing
        @State private var points: [CGPoint] = []
        @State private var started = Date.now

        var body: some View {
            Canvas { context, size in
                let all = CGRect(origin: .zero, size: size)
                if !drawing.strokes.isEmpty, let ink = lightPicture(of: drawing, in: all) {
                    context.draw(Image(decorative: ink, scale: pictureScale), in: all)
                }
                var path = Path()
                path.addLines(points)
                context.stroke(
                    path, with: .color(.black),
                    style: StrokeStyle(lineWidth: 3, lineCap: .round, lineJoin: .round))
            }
            .background(.white)
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in
                        if points.isEmpty { started = .now }
                        points.append(value.location)
                    }
                    .onEnded { _ in addStroke() }
            )
        }

        private func addStroke() {
            guard !points.isEmpty else { return }
            let controlPoints = points.enumerated().map { index, point in
                PKStrokePoint(
                    location: point, timeOffset: TimeInterval(index) / 60,
                    size: CGSize(width: 3, height: 3), opacity: 1, force: 1, azimuth: 0,
                    altitude: .pi / 2)
            }
            let path = PKStrokePath(controlPoints: controlPoints, creationDate: started)
            drawing.strokes.append(PKStroke(ink: PKInk(.pen, color: .black), path: path))
            points = []
        }
    }
#endif
