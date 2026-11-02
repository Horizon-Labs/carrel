// Handwritten notes on cards (FORMAT.md §5.10): drawings saved with Apple Pencil on iPad, or
// the pointer on a Mac, kept beside the card with a picture and the text recognised in them.
// Each save or removal is one change undo can reverse, so History follows it.

import Foundation

extension ArchiveModel {
    /// The handwritten notes on the item `id`'s card, oldest first.
    public func drawings(of id: String) async throws -> [CardDrawingInfo] {
        try await call { try $0.cardDrawings(item: id) }
    }

    /// Saves a handwritten note on the item `id`'s card: `data` is the `PKDrawing`'s data
    /// representation, `picture` a PNG of it, and `text` what was recognised in it. A new
    /// drawing when `drawing` is `nil`, else a new version of it.
    @discardableResult
    public func saveDrawing(
        on id: String, drawing: String?, data: Data, picture: Data, text: String
    ) async throws -> CardDrawingChange {
        try await afterDrawing(
            try await call {
                try $0.saveCardDrawing(
                    item: id, drawing: drawing, data: data, picture: picture, text: text)
            })
    }

    /// Removes the handwritten note `drawing` from the item `id`'s card.
    @discardableResult
    public func removeDrawing(_ drawing: String, from id: String) async throws
        -> CardDrawingChange
    {
        try await afterDrawing(
            try await call { try $0.removeCardDrawing(item: id, drawing: drawing) })
    }

    /// History shows a drawing that changed something.
    private func afterDrawing(_ change: CardDrawingChange) async throws -> CardDrawingChange {
        if change.changed, status?.hasHistory == true { try await refreshHistory() }
        return change
    }
}
