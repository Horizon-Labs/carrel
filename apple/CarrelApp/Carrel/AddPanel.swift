// Adding is a mode, not a screen (docs/design/library-home.md): the + button, File → Add
// Items… (⌘N) and an `ingest` link open the Add panel, with the ways to add and the intake queue
// that Status used to show. Choosing a way closes the panel first, then opens its sheet, since
// a window shows one sheet at a time. Status keeps one line saying what's being added.

import CarrelKit
import SwiftUI

/// The Add panel: files, a web address, a note or the clipboard, and the intake queue with
/// the worker's progress.
struct AddPanel: View {
    let model: ArchiveWindowModel
    let intake: IntakeModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                if !model.isReadOnly {
                    Section {
                        Button("Files and Folders…", systemImage: "folder") {
                            then { chooseFilesToAdd(intake) }
                        }
                        Button("Web Address…", systemImage: "link") {
                            then { intake.writing = Writing(kind: .webAddress) }
                        }
                        Button("Note…", systemImage: "note.text") {
                            then { intake.writing = Writing(kind: .note) }
                        }
                        Button("From the Clipboard", systemImage: "doc.on.clipboard") {
                            then { Task { await intake.paste(from: .general) } }
                        }
                    } header: {
                        Text("Add to the Archive")
                    } footer: {
                        Text(
                            isMac
                                ? "You can also drop files, folders and web addresses anywhere on the window."
                                : "Photos, Take Photo and Scan Document are in the + menu."
                        )
                    }
                }
                IntakeSection(intake: intake, isReadOnly: model.isReadOnly)
            }
            .formStyle(.grouped)
            .navigationTitle("Add Items")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .sheetWidth(480, height: 560)
        .feedbackScreen(step: "add panel")
    }

    /// Closes the panel, then does `action`.
    private func then(_ action: @escaping () -> Void) {
        model.afterAdd = action
        dismiss()
    }
}

/// Asks for files and folders, then starts adding them with the add sheet.
@MainActor
func chooseFilesToAdd(_ intake: IntakeModel) {
    FileChoosing.shared.choose(
        .init(
            folders: true, files: true, multiple: true, prompt: "Add",
            message: "Choose files and folders to add to the intake queue."
        ) { urls in Task { await intake.add(urls) } })
}

/// What's being added, in one line for Status: the worker's progress, else how many wait.
struct IntakeStatusLine: View {
    let model: ArchiveWindowModel
    let intake: IntakeModel

    var body: some View {
        Section("Adding") {
            LabeledContent {
                Button("Show Queue") { model.showingAdd = true }
            } label: {
                Text(line).lineLimit(1).truncationMode(.middle)
            }
        }
    }

    private var line: String {
        let waiting = intake.queue.waiting.count
        let failed = intake.queue.failed.count
        var text =
            intake.workerLine
            ?? (waiting == 0 ? "Nothing is being added." : "\(waiting) waiting to be drafted.")
        if failed > 0 { text += " \(failed) failed." }
        return text
    }
}

extension View {
    /// The Add panel, and on the Mac the + button that opens it.
    func addPanel(_ model: ArchiveWindowModel) -> some View {
        modifier(AddPanelPresenter(model: model))
    }
}

private struct AddPanelPresenter: ViewModifier {
    @Bindable var model: ArchiveWindowModel

    func body(content: Content) -> some View {
        content
            #if os(macOS)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Add Items", systemImage: "plus") { model.showingAdd = true }
                .help("Add files, a web address or a note, and see what’s being added (⌘N)")
                .disabled(model.intake == nil)
            }
        }
            #endif
            .sheet(isPresented: $model.showingAdd, onDismiss: afterClosing) {
                if let intake = model.intake { AddPanel(model: model, intake: intake) }
            }
    }

    /// Does what was chosen in the panel, once it has closed.
    private func afterClosing() {
        let action = model.afterAdd
        model.afterAdd = nil
        action?()
    }
}
