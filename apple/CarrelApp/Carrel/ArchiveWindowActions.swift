// What an archive's window does beyond its screens: search (⌘F; in the toolbar on the Mac, in
// ToolbarSearchField.swift), adding items by dropping them on the window or with File → Add to
// Archive… (⌘O), and watching the worker draft them.

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
                Task { await intake.add(urls) }
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
            .modifier(AddFromWindowSheets(model: model))
            .task(id: model.intake.map(ObjectIdentifier.init)) {
                await model.intake?.watch()
            }
    }
}

/// The commands for the archive window in front: Add to Archive…, the other ways to add, and
/// Search.
struct ArchiveWindowCommands: Commands {
    @FocusedValue(ArchiveWindowModel.self) private var window

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Divider()
            Button("Add to Archive…") {
                guard let intake = window?.intake else { return }
                FileChoosing.shared.choose(
                    .init(
                        folders: true, files: true, multiple: true, prompt: "Add",
                        message: "Choose files and folders to add to the intake queue."
                    ) { urls in Task { await intake.add(urls) } })
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
