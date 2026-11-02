// Send Feedback (#487), from any screen: Help → Send Feedback (⌘⇧F) and the toolbar's overflow
// on the Mac, shaking or the ••• menu on iPad and iPhone. The form shows where it's about and
// the window's screenshot, which is easy to leave out, and sends with ⌘↩. It's kept in the
// archive for development; nothing is posted.

import CarrelKit
import Combine
import SwiftUI

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The feedback form's state, while it's open.
@Observable
final class FeedbackModel: Identifiable {
    let id = UUID()
    let screen: FeedbackScreen
    let screenshot: Data?
    var kind = "bug"
    var words = ""
    var includeScreenshot: Bool

    init(screen: FeedbackScreen, screenshot: Data?) {
        self.screen = screen
        self.screenshot = screenshot
        includeScreenshot = screenshot != nil
    }

    /// Where it's about, in one line: `Review → proposal`.
    var about: String {
        screen.step.isEmpty ? screen.screen : "\(screen.screen) → \(screen.step)"
    }

    var canSend: Bool { !words.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    /// What the app sends.
    var sending: FeedbackSending {
        FeedbackSending(
            kind: kind, words: words, screen: screen, interface: Self.interface,
            deviceName: Self.deviceName, appVersion: Self.appVersion,
            screenshot: includeScreenshot ? screenshot : nil)
    }

    static var interface: String {
        #if os(macOS)
        "mac app"
        #else
        UIDevice.current.userInterfaceIdiom == .pad ? "ipad app" : "iphone app"
        #endif
    }

    /// The kind of device and its model identifier, such as `iPad (iPad16,3)`; never its name.
    static var deviceName: String {
        var size = 0
        #if os(macOS)
        let key = "hw.model"
        let kind = "Mac"
        #else
        let key = "hw.machine"
        let kind = UIDevice.current.model
        #endif
        if let simulated = ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"] {
            return "\(kind) (\(simulated), Simulator)"
        }
        sysctlbyname(key, nil, &size, nil, 0)
        guard size > 0 else { return kind }
        var bytes = [CChar](repeating: 0, count: size)
        sysctlbyname(key, &bytes, &size, nil, 0)
        let model = String(decoding: bytes.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
        return model.isEmpty ? kind : "\(kind) (\(model))"
    }

    static var appVersion: String? {
        let info = Bundle.main.infoDictionary
        guard let version = info?["CFBundleShortVersionString"] as? String else { return nil }
        let build = info?["CFBundleVersion"] as? String
        return build.map { "\(version) (\($0))" } ?? version
    }

    /// The window in front, as PNG, or `nil` if it can't be drawn.
    static func windowScreenshot() -> Data? {
        #if os(macOS)
        guard let window = NSApp.keyWindow ?? NSApp.mainWindow,
            let view = window.contentView?.superview ?? window.contentView,
            let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds)
        else { return nil }
        view.cacheDisplay(in: view.bounds, to: rep)
        return rep.representation(using: .png, properties: [:])
        #else
        let window = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }?
            .keyWindow
        guard let window else { return nil }
        let renderer = UIGraphicsImageRenderer(bounds: window.bounds)
        return renderer.pngData { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: false)
        }
        #endif
    }
}

struct FeedbackSheet: View {
    let window: ArchiveWindowModel
    @Bindable var model: FeedbackModel

    @Environment(\.dismiss) private var dismiss
    @State private var problem: String?
    @State private var sending = false
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Send Feedback").font(.headline)
            Text(
                "About \(model.about). It’s saved in the archive for development, with the "
                    + "screen, Carrel’s version and recent problems, never contents. Nothing is "
                    + "posted."
            )
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
            Picker("Kind", selection: $model.kind) {
                Text("Bug").tag("bug")
                Text("Idea").tag("idea")
                Text("Confusing").tag("confusing")
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            TextField("What happened, in a sentence or two", text: $model.words, axis: .vertical)
                .lineLimit(3...8)
                .textFieldStyle(.roundedBorder)
                .focused($focused)
            if let data = model.screenshot, let image = Self.image(data) {
                HStack(alignment: .top, spacing: 12) {
                    image
                        .resizable()
                        .scaledToFit()
                        .frame(maxWidth: 160, maxHeight: 110)
                        .clipShape(.rect(cornerRadius: 4))
                        .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(.separator))
                        .opacity(model.includeScreenshot ? 1 : 0.3)
                    Toggle("Include the screenshot", isOn: $model.includeScreenshot)
                        .checkboxToggleStyle()
                }
            }
            if let problem {
                Label(problem, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
            HStack {
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button("Send") { Task { await send() } }
                    .keyboardShortcut(.return, modifiers: .command)
                    .buttonStyle(.borderedProminent)
                    .disabled(!model.canSend || sending)
            }
        }
        .padding(20)
        .sheetWidth(480)
        .onAppear { focused = true }
    }

    private func send() async {
        sending = true
        defer { sending = false }
        do {
            try await window.archive?.sendFeedback(model.sending)
            dismiss()
        } catch {
            problem = error.localizedDescription
        }
    }

    private static func image(_ data: Data) -> Image? {
        #if os(macOS)
        NSImage(data: data).map { Image(nsImage: $0) }
        #else
        UIImage(data: data).map { Image(uiImage: $0) }
        #endif
    }
}

/// The form's sheet, and on iPad and iPhone, shaking to open it.
private struct FeedbackActions: ViewModifier {
    @Bindable var model: ArchiveWindowModel

    func body(content: Content) -> some View {
        content
            .environment(model.feedbackMarks)
            .sheet(item: $model.feedback) { FeedbackSheet(window: model, model: $0) }
            #if os(iOS)
            .onReceive(NotificationCenter.default.publisher(for: .deviceDidShake)) { _ in
                model.startFeedback()
            }
            #endif
    }
}

/// Send Feedback in a column's toolbar overflow (the ••• menu on iPad and iPhone).
private struct FeedbackToolbarItem: ViewModifier {
    let model: ArchiveWindowModel

    func body(content: Content) -> some View {
        content.toolbar {
            ToolbarItem(placement: .secondaryAction) {
                Button("Send Feedback…", systemImage: "exclamationmark.bubble") {
                    model.startFeedback()
                }
                .disabled(model.archive == nil)
            }
        }
    }
}

extension View {
    /// The window's feedback form, and shaking to open it on iPad and iPhone.
    func feedbackActions(_ model: ArchiveWindowModel) -> some View {
        modifier(FeedbackActions(model: model))
    }

    /// Send Feedback in this column's toolbar overflow.
    func feedbackToolbarItem(_ model: ArchiveWindowModel) -> some View {
        modifier(FeedbackToolbarItem(model: model))
    }
}

/// Help → Send Feedback (⌘⇧F), for the window in front.
struct FeedbackCommands: Commands {
    @FocusedValue(ArchiveWindowModel.self) private var window

    var body: some Commands {
        CommandGroup(after: .help) {
            Button("Send Feedback…") { window?.startFeedback() }
                .keyboardShortcut("f", modifiers: [.command, .shift])
                .disabled(window?.archive == nil)
        }
    }
}

#if os(iOS)
extension Notification.Name {
    /// The device was shaken, which opens the feedback form.
    static let deviceDidShake = Notification.Name("CarrelDeviceDidShake")
}

extension UIWindow {
    open override func motionEnded(_ motion: UIEvent.EventSubtype, with event: UIEvent?) {
        if motion == .motionShake {
            NotificationCenter.default.post(name: .deviceDidShake, object: self)
        }
    }
}
#endif
