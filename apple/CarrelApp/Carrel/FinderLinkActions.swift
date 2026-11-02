// What a link from the Finder extension asks the window to start (docs/design/macos-integration.md,
// "Finder"): an action that needs a decision opens the app's own sheet for it rather than deciding
// in Finder. Check Out… asks where to and when it's due, Return… shows the return's preview,
// Cancel Checkout… asks first, Ask About This opens the librarian about the item, and Write a Note opens the drawing editor
// on its card.

import CarrelKit
import Foundation

extension ArchiveWindowModel {
    /// Starts `action` on the item `id`, in the sheet it needs.
    func start(_ action: AgentLink.Action, on id: String) async {
        switch action {
        case .checkOut:
            screen = .active
            let title = items.first { $0.id == id }?.title ?? id
            activeScreen?.startCheckout(of: id, titled: title)
        case .returnItem:
            screen = .active
            activeScreen?.selection = [.item(id)]
            await activeScreen?.askToReturn()
        case .cancelCheckout:
            screen = .active
            activeScreen?.selection = [.item(id)]
            activeScreen?.askToCancel()
        case .ask:
            if let item = items.first(where: { $0.id == id }) { librarian?.ask(about: item) }
        case .draw:
            open(item: id)
            // After Browse has replaced the screen the link showed first, so the editor opens
            // on the card that stays.
            try? await Task.sleep(for: .milliseconds(500))
            WritesNotesFromLinks.writeNote(on: id)
        }
    }
}
