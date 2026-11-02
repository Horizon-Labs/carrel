// The inspector's Drawings section: handwritten notes on an item's card (FORMAT.md §5.10),
// each shown by its picture and the text recognised in it, as `carrel show` lists them. On
// iPad they're drawn with Apple Pencil; on a Mac with the pointer. Each save or removal is
// one change undo can reverse.

import CarrelKit
import ImageIO
import SwiftUI

struct ItemDrawingsSection: View {
    /// The item a link asked to write a note on (`action=draw`), opened when its card shows.
    static var noteToWrite: String?

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
        .task(id: item) {
            if Self.noteToWrite == item {
                Self.noteToWrite = nil
                editing = EditedDrawing(existing: nil)
            }
            await reload()
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
