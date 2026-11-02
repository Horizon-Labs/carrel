// Spotlight on iPad and iPhone (docs/design/sync-and-mobile.md, "iPad and iPhone"): the app
// indexes the open archive's cards, collections and glossary facts itself, as the agent does on
// the Mac and by the same `spotlight.index` setting, when the archive opens, after its items
// change and in the background tasks. A result opens through its `carrel://` link.

#if os(iOS)
import CarrelKit
import CoreSpotlight
import Foundation

/// The app's own Spotlight index, kept in step with the archive that's open.
enum MobileSpotlight {
    /// One indexer for the app's life, so passes after the first index only what changed.
    @MainActor private static let indexer = SpotlightIndexer(index: CoreSpotlightIndex())

    /// Brings what Spotlight finds in the archive `handle` up to date, and waits for it.
    @MainActor
    static func refresh(_ handle: ArchiveHandle) async {
        guard !CarrelMain.isHostingTests else { return }
        indexer.sync(
            handle, root: URL(filePath: handle.root(), directoryHint: .isDirectory), at: .now,
            changed: true)
        await indexer.settled()
    }

    /// The link a chosen Spotlight result opens, with its archive moved to where the app's
    /// container is now; `nil` for another activity.
    static func link(for activity: NSUserActivity) -> URL? {
        guard var link = AgentLink(spotlightResult: activity) else { return nil }
        link.archive = MobileLibrary.current(link.archive)
        return link.url
    }
}
#endif
