// What an archive's window does beyond its screens: search (⌘F; in the toolbar on the Mac, in
// ToolbarSearchField.swift), adding items by dropping them on the window, straight away with a
// banner saying what went in, or with File → Add to Archive… (⌘O), and watching the worker draft
// them.

import CarrelKit
import SwiftUI

/// Search on iPad and iPhone, adding items by dropping them, and the window's commands.
struct ArchiveWindowActions: ViewModifier {
    let model: ArchiveWindowModel
    @State private var isTargeted = false
    #if os(iOS)
    @FocusState private var searchFocused: Bool
    #endif

    func body(content: Content) -> some View {
        content
            #if os(iOS)
        .searchable(
            text: Binding(
                get: { model.search?.text ?? "" },
                set: { model.search?.text = $0 }),
            placement: .toolbar,
            prompt: "Search, or tag:receipts"
        )
        .searchFocused($searchFocused)
        .onChange(of: model.search?.focusRequests) { searchFocused = true }
            #endif
            .onChange(of: model.screen) { model.search?.clear() }
            .dropDestination(for: URL.self) { urls, _ in
                guard let intake = model.intake, !model.isReadOnly else { return false }
                Task { await intake.drop(urls) }
                return true
            } isTargeted: {
                isTargeted = $0
            }
            .overlay {
                if isTargeted, !model.isReadOnly {
                    RoundedRectangle(cornerRadius: 8)
                        .strokeBorder(.tint, lineWidth: 3)
                        .overlay {
                            Label("Add to the Intake Queue", systemImage: "tray.and.arrow.down")
                                .font(.title2)
                                .padding()
                                .background(.regularMaterial, in: .rect(cornerRadius: 8))
                        }
                        .allowsHitTesting(false)
                }
            }
            .alert(
                model.intake?.report?.title ?? "",
                isPresented: Binding(
                    get: { model.intake?.report != nil },
                    set: { if !$0 { model.intake?.report = nil } }),
                presenting: model.intake?.report
            ) { _ in
            } message: { report in
                if let details = report.details { Text(details) }
            }
            .overlay(alignment: .bottom) {
                if let intake = model.intake, let dropped = intake.dropped {
                    DropBanner(report: dropped) {
                        model.showingAdd = true
                        intake.dropped = nil
                    } close: {
                        intake.dropped = nil
                    }
                    .padding()
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .animation(.default, value: model.intake?.dropped)
            .modifier(AddFromWindowSheets(model: model))
            .task(id: model.intake.map(ObjectIdentifier.init)) {
                await model.intake?.watch()
            }
    }
}

/// The commands for the archive window in front: Add Items… (the Add panel), Add to Archive…,
/// the other ways to add, and Search.
struct ArchiveWindowCommands: Commands {
    @FocusedValue(ArchiveWindowModel.self) private var window

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Divider()
            Button("Add Items…") { window?.showingAdd = true }
                .keyboardShortcut("n")
                .disabled(window?.intake == nil)
            Button("Add to Archive…") {
                guard let intake = window?.intake else { return }
                chooseFilesToAdd(intake)
            }
            .keyboardShortcut("o")
            .disabled(window?.intake == nil || window?.isReadOnly != false)
            AddFromWindowCommands(window: window)
        }
        CommandGroup(before: .textEditing) {
            Button("Search Archive") { window?.search?.focus() }
                .keyboardShortcut("f")
                .disabled(window?.search == nil)
        }
    }
}

/// What the drops added, at the foot of the window, where it doesn't stop the next drop. It
/// goes by itself after a few seconds unless something was left out or went wrong.
private struct DropBanner: View {
    let report: AddReport
    let showQueue: () -> Void
    let close: () -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Image(systemName: report.needsReading ? "exclamationmark.triangle" : "tray.and.arrow.down")
                .foregroundStyle(report.needsReading ? .orange : .secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(report.title).font(.headline)
                if let details = report.details {
                    Text(details).font(.callout).foregroundStyle(.secondary).lineLimit(3)
                }
            }
            Spacer(minLength: 0)
            Button("Show Queue", action: showQueue)
            Button("Close", systemImage: "xmark", action: close)
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
        }
        .padding(12)
        .frame(maxWidth: 520)
        .background(.regularMaterial, in: .rect(cornerRadius: 10))
        .shadow(radius: 4, y: 2)
        .task(id: report) {
            guard !report.needsReading else { return }
            try? await Task.sleep(for: .seconds(6))
            if !Task.isCancelled { close() }
        }
        .feedbackScreen(step: "drop banner")
    }
}
