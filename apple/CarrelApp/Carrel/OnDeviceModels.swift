// The models an iPad or iPhone drafts with (docs/design/sync-and-mobile.md, "iPad and
// iPhone"; #481): Apple's model through Foundation Models, or a small model through MLX, run
// in the app's own process, since iOS has no provider programs. They're the same code the Mac
// runs as `carrel-provider-apple` and `carrel-provider-mlx`. The core decides which to try
// first (`interimOnDeviceOrder`, until the device's self-benchmark, #469, ranks them), and
// the battery rules (`PowerPolicy`) decide when heavy work like drafting runs at all.

import CarrelKit
import Foundation

#if os(iOS)
import CarrelAppleProviders
import CarrelMLXProvider
import os
#endif

/// The app's model for the core on iPad and iPhone: the on-device models, best first, each
/// tried in turn when one can't take a prompt, as the core tries providers in turn.
final class OnDeviceModels: ModelProvider, @unchecked Sendable {
    #if os(iOS)
    /// The models to try, best first.
    let models: [any CarrelAppleProviders.TextModel]

    init(models: [any CarrelAppleProviders.TextModel]) {
        self.models = models
    }
    #endif

    /// The on-device models for `handle`'s device, or `nil` on a Mac, which runs them as
    /// provider programs instead. Reads the models folder, so call it off the main thread
    /// when it might be large.
    nonisolated static func forArchive(_ handle: ArchiveHandle) -> OnDeviceModels? {
        #if os(iOS)
        let apple = FoundationModel()
        let installed = handle.modelsFolder()
            .map { ModelFolder.discover(in: URL(filePath: $0, directoryHint: .isDirectory)) }
            ?? []
        let available = availableMemory
        let order = interimOnDeviceOrder(
            appleAvailable: apple.offer.available, available: available,
            installed: installed.map { InstalledMlxModel(name: $0.name, size: $0.size) })
        let models: [any CarrelAppleProviders.TextModel] = order.compactMap { choice in
            guard let name = choice.mlx else { return apple }
            return installed.first { $0.name == name }
                .map { MLXModel(folder: $0, memory: available) }
        }
        return OnDeviceModels(models: models)
        #else
        _ = handle
        return nil
        #endif
    }

    #if os(iOS)
    /// The memory this app may still use: iOS ends an app that goes past it, so a model must
    /// fit in it. The Simulator has no such limit, so it counts half the Mac's.
    nonisolated static var availableMemory: UInt64 {
        let limit = UInt64(os_proc_available_memory())
        return limit > 0 ? limit : ProcessInfo.processInfo.physicalMemory / 2
    }
    #endif

    func complete(request: ModelRequest) throws -> ModelAnswer {
        #if os(iOS)
        var reasons: [String] = []
        for model in models {
            switch Self.wait(for: model, request) {
            case .success(let completion):
                return ModelAnswer(text: completion.text, model: completion.model)
            case .failure(.unavailable(let why)), .failure(.unsupported(let why)):
                reasons.append("\(model.offer.label ?? model.offer.id): \(why)")
            case .failure(let error):
                throw CarrelError.Callback(message: "\(error)")
            }
        }
        throw CarrelError.Callback(
            message: reasons.isEmpty
                ? "No model on this device can draft: turn on Apple Intelligence, or download a model in Settings, under Models."
                : reasons.joined(separator: "; "))
        #else
        _ = request
        throw CarrelError.Callback(message: "the Mac's models run as provider programs")
        #endif
    }

    #if os(iOS)
    /// Runs `model` on `request` and waits: the core calls from its own worker thread, never
    /// the main one, and waits for the answer.
    private static func wait(
        for model: any CarrelAppleProviders.TextModel, _ request: ModelRequest
    ) -> Result<Completion, ProviderError> {
        let asked = CompletionRequest(
            system: request.system, prompt: request.prompt,
            schema: request.schema.flatMap { JSONValue(parsing: $0) },
            maxTokens: request.maxTokens.map(Int.init))
        nonisolated(unsafe) var result: Result<Completion, ProviderError> =
            .failure(.failed("no answer"))
        let done = DispatchSemaphore(value: 0)
        Task.detached(priority: .userInitiated) {
            do throws(ProviderError) {
                result = .success(try await model.complete(asked))
            } catch {
                result = .failure(error)
            }
            done.signal()
        }
        done.wait()
        return result
    }
    #endif
}

#if os(iOS)
import SwiftUI

/// Settings → This Device's Drafting section: which models draft here, in the order they're
/// tried, or why none can.
struct OnDeviceDraftingSection: View {
    let archive: ArchiveModel?
    @State private var names: [String]?
    @State private var apple: String?

    var body: some View {
        Section {
            if let names, !names.isEmpty {
                ForEach(Array(names.enumerated()), id: \.offset) { index, name in
                    LabeledContent(index == 0 ? "Drafts with" : "Then", value: name)
                }
            } else if names != nil {
                Text(
                    "No model here can draft. Turn on Apple Intelligence in the Settings app, or download a model under Models."
                )
                .foregroundStyle(.secondary)
            }
            if let apple { Text(apple).font(.caption).foregroundStyle(.secondary) }
        } header: {
            Text("Drafting")
        } footer: {
            Text(
                "Apple’s model comes first when Apple Intelligence is on, then downloaded MLX models that fit in the memory \(thisDevice) gives Carrel, largest first."
            )
        }
        .task(id: archive.map(ObjectIdentifier.init)) {
            guard let handle = archive?.handle else { return }
            let found = await Task.detached {
                let models = OnDeviceModels.forArchive(handle)?.models ?? []
                let offer = FoundationModel().offer
                return (models.map { $0.offer.label ?? $0.offer.id }, offer.reason)
            }.value
            names = found.0
            apple = found.1.map { "Apple’s model: \($0)." }
        }
    }
}
#endif
