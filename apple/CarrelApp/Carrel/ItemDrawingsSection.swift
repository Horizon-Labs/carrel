// The inspector's Drawings section: handwritten notes on an item's card (FORMAT.md §5.10),
// each shown by its picture and the text recognised in it, as `carrel show` lists them. On
// iPad they're drawn with Apple Pencil; on a Mac with the pointer. Each save or removal is
// one change undo can reverse.

import CarrelKit
import ImageIO
import SwiftUI

struct ItemDrawingsSection: View {
    /// Posted when a drawing on `object`'s card (the item's identifier) was saved elsewhere,
    /// such as from a link.
    static let drawingsChanged = Notification.Name("CarrelDrawingsChanged")

    let archive: ArchiveModel
    let item: String
    @State private var drawings: [CardDrawingInfo]?
    /// The drawing open in the editor: a new one, or one of `drawings`.
    @State private var editing: EditedDrawing?
    @State private var problem: String?

    var body: some View {
        Section("Drawings") {
            if let drawings {
                ForEach(drawings, id: \.id) { drawing in
                    Button {
                        editing = EditedDrawing(existing: drawing)
                    } label: {
                        DrawingRow(drawing: drawing)
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint("Opens the drawing to change it")
                    .contextMenu {
                        Button("Remove Drawing", systemImage: "trash", role: .destructive) {
                            Task { await remove(drawing) }
                        }
                    }
                }
                Button(isMac ? "Draw a Note" : "Write a Note", systemImage: "pencil.tip") {
                    editing = EditedDrawing(existing: nil)
                }
                if let problem {
                    Text(problem).font(.caption).foregroundStyle(.red)
                }
            } else {
                ProgressView().frame(maxWidth: .infinity)
            }
        }
        .task(id: item) { await reload() }
        .onReceive(NotificationCenter.default.publisher(for: Self.drawingsChanged)) { note in
            if note.object as? String == item { Task { await reload() } }
        }
        .sheet(item: $editing) { edited in
            DrawingEditor(existing: edited.existing) { drawn in
                try await save(drawn, over: edited.existing)
            }
        }
    }

    private func reload() async {
        drawings = (try? await archive.drawings(of: item)) ?? []
    }

    private func save(_ drawn: DrawnNote, over existing: CardDrawingInfo?) async throws {
        try await archive.saveDrawing(
            on: item, drawing: existing?.id, data: drawn.data, picture: drawn.picture,
            text: drawn.text)
        problem = nil
        await reload()
    }

    private func remove(_ drawing: CardDrawingInfo) async {
        do {
            try await archive.removeDrawing(drawing.id, from: item)
            problem = nil
        } catch {
            problem = (error as? CarrelError)?.message ?? error.localizedDescription
        }
        await reload()
    }
}

/// Opens the editor for a new note when a link asks for one (`action=draw`), on the card as a
/// whole, since the Drawings section's rows aren't made until they're scrolled to.
struct WritesNotesFromLinks: ViewModifier {
    /// The item a link asked to write a note on, taken by its card when it shows.
    private static var noteToWrite: String?
    private static let writeNoteAsked = Notification.Name("CarrelWriteNote")

    /// Opens the editor for a new note on the item `id`'s card, now if it's showing, or else
    /// when it shows.
    static func writeNote(on id: String) {
        noteToWrite = id
        NotificationCenter.default.post(name: writeNoteAsked, object: nil)
    }

    let archive: ArchiveModel?
    let item: String
    @State private var writing = false

    func body(content: Content) -> some View {
        content
            .task(id: item) { take() }
            .onReceive(NotificationCenter.default.publisher(for: Self.writeNoteAsked)) { _ in
                take()
            }
            .sheet(isPresented: $writing) {
                DrawingEditor(existing: nil) { drawn in
                    guard let archive else { return }
                    try await archive.saveDrawing(
                        on: item, drawing: nil, data: drawn.data, picture: drawn.picture,
                        text: drawn.text)
                    NotificationCenter.default.post(
                        name: ItemDrawingsSection.drawingsChanged, object: item)
                }
            }
    }

    private func take() {
        guard archive != nil, Self.noteToWrite == item else { return }
        Self.noteToWrite = nil
        writing = true
    }
}

/// A drawing open in the editor.
private struct EditedDrawing: Identifiable {
    let id = UUID()
    /// The drawing it changes; `nil` for a new one.
    let existing: CardDrawingInfo?
}

/// One drawing: its picture, and the text recognised in it.
private struct DrawingRow: View {
    let drawing: CardDrawingInfo

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let picture = loadPicture(drawing.picturePath) {
                Image(decorative: picture, scale: 2)
                    .resizable()
                    .scaledToFit()
                    .frame(maxHeight: 240)
                    .clipShape(.rect(cornerRadius: 6))
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
            } else {
                Label("Its picture isn’t here yet.", systemImage: "icloud.and.arrow.down")
                    .foregroundStyle(.secondary)
            }
            if let text = drawing.text {
                Text(text).font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
            }
            if let when = drawing.modified ?? drawing.at {
                Text(String(when.prefix(10))).font(.caption2).foregroundStyle(.tertiary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(.rect)
    }
}

/// The PNG at `path`, if it can be read.
private func loadPicture(_ path: String) -> CGImage? {
    guard let source = CGImageSourceCreateWithURL(URL(filePath: path) as CFURL, nil) else {
        return nil
    }
    return CGImageSourceCreateImageAtIndex(source, 0, nil)
}
