// The screen a `carrel://open?screen=` link asks for, from `carrel gui SCREEN`, shown in the
// archive's window by the names the interactive mode uses.

import CarrelKit

extension ArchiveWindowModel {
    /// Shows the screen a link asked for. The window has no Ingest or Librarian screen:
    /// Ingest is the Add panel, where the intake queue is, and the librarian is its panel. Settings
    /// is its own window, which the archive's window opens.
    func show(linked screen: AgentLink.Screen) {
        switch screen {
        case .library: self.screen = .library
        case .status: self.screen = .status
        case .ingest: showingAdd = true
        case .review: self.screen = .review
        case .active: self.screen = .active
        case .questions: self.screen = .questions
        case .history: self.screen = .history
        case .backups: self.screen = .backups
        case .browse:
            if let first = tree.first { self.screen = .collection(first.collection.path) }
        case .discover(let page):
            // Discover's pages are views of the Library now; On This Day is on its covers.
            switch page {
            case .wander?: self.screen = .discover(.wander)
            case .timeline?, .map?, .activity?, .graph?:
                if let page, let view = archiveViewNamed(name: page.rawValue) {
                    show(view, of: nil)
                }
                self.screen = .library
            case .today?, nil:
                show(.covers, of: nil)
                self.screen = .library
            }
        case .librarian: librarian?.show()
        case .settings: break
        }
    }
}
