// What an archive's window shows: the screens in the sidebar, the collection tree, the chosen
// screen's list and the chosen item's card.
//
// It reads the archive through the bindings CarrelKit has today (collections and items).
// CarrelKit's ArchiveModel (#233) takes over loading and the other screens once it lands.

import CarrelKit
import Foundation
import Observation

/// A screen in the sidebar, in the interactive mode's order (docs/design/library-home.md).
enum Screen: Hashable {
    /// What the archive opens on: its collections, what's new and what's waiting.
    case library
    case status
    case review
    case questions
    /// Restructure proposals.
    case restructure
    /// Checked Out: what's checked out and on the desk.
    case active
    /// What's flagged as needing work.
    case flagged
    /// Browse, showing the collection at this path.
    case collection(String)
    case history
    case backups
    /// A page of Discover.
    case discover(DiscoverPage)
}

/// A collection and the collections nested inside it.
struct CollectionNode: Identifiable, Hashable {
    let collection: CollectionSummary
    /// `nil` when it has none, so the sidebar shows no disclosure triangle.
    let children: [CollectionNode]?
    var id: String { collection.path }

    /// The tree of `collections`, from paths such as `Projects/Hardware`.
    static func tree(_ collections: [CollectionSummary]) -> [CollectionNode] {
        let paths = Set(collections.map(\.path))
        func parent(_ path: String) -> String? {
            var components = path.split(separator: "/")
            while components.count > 1 {
                components.removeLast()
                let candidate = components.joined(separator: "/")
                if paths.contains(candidate) { return candidate }
            }
            return nil
        }
        let byParent = Dictionary(grouping: collections) { parent($0.path) }
        func nodes(under path: String?) -> [CollectionNode] {
            (byParent[path] ?? [])
                .sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
                .map { collection in
                    let children = nodes(under: collection.path)
                    return CollectionNode(
                        collection: collection, children: children.isEmpty ? nil : children)
                }
        }
        return nodes(under: nil)
    }
}

/// What Status shows until the core's Status summary reaches CarrelKit (#233).
struct ArchiveSummary: Equatable {
    var itemCount: Int
    var collectionCount: Int
    /// Item counts by kind, most common first.
    var kinds: [(kind: ItemKind, count: Int)]
    var holdingCount: Int

    static func == (a: Self, b: Self) -> Bool {
        a.itemCount == b.itemCount && a.collectionCount == b.collectionCount
            && a.holdingCount == b.holdingCount
            && a.kinds.map(\.kind) == b.kinds.map(\.kind)
            && a.kinds.map(\.count) == b.kinds.map(\.count)
    }
}

@Observable
final class ArchiveWindowModel {
    let url: URL
    /// How the archive is opened; `nil` for the device's own configuration.
    let options: OpenOptions?
    private(set) var handle: ArchiveHandle?
    private(set) var name: String
    private(set) var isReadOnly = false
    private(set) var collections: [CollectionSummary] = []
    private(set) var tree: [CollectionNode] = []
    private(set) var items: [ItemSummary] = []
    private(set) var isLoading = false
    private(set) var problem: String?
    /// The archive's queues and operations, through the core, once it's open.
    private(set) var archive: ArchiveModel?

    // Search and adding items (#236).
    /// Search in the toolbar.
    private(set) var search: SearchModel?
    /// Adding items, and the worker drafting them.
    private(set) var intake: IntakeModel?

    // Review, Questions, corrections and editing (#234).
    private(set) var review: ReviewModel?
    private(set) var questions: QuestionsModel?
    private(set) var backups: BackupsModel?
    /// The correction sheet's, while it's open.
    var correction: CorrectionModel?
    /// Report a Missing Feature's sheet, while it's open (#455).
    var missingFeature: MissingFeatureModel?
    /// Everything flagged, with where the librarian is with each (`FlagLibrarian.swift`).
    var flagRequests: [FlagRequestView] = []
    /// Send Feedback's form, while it's open, and where each view says the owner is (#487).
    var feedback: FeedbackModel?
    let feedbackMarks = FeedbackMarks()
    /// The filed item whose card the inspector is editing (⌘E).
    var editing: CardEditModel?
    /// Move to iCloud Drive…'s sheet, while it's open.
    var icloudMove: ICloudMoveModel?

    // Restructure proposals (#327).
    private(set) var restructure: RestructureModel?
    /// The crawler's findings in Review, and its changes on Status.
    private(set) var crawler: CrawlerModel?
    /// Every device's capability profile and the re-check queue, on Status.
    private(set) var devices: DevicesModel?

    // Active and History (#235).
    private(set) var activeScreen: ActiveScreenModel?
    private(set) var historyScreen: HistoryScreenModel?

    // Browse's actions: new collections, renaming and expanding (#268).
    private(set) var browse: BrowseActionsModel?

    // The librarian and Discover (#291).
    private(set) var librarian: LibrarianModel?
    private(set) var discover: DiscoverModel?

    // The Library (#512).
    /// The shelves and what's new, from the core.
    private(set) var library: ArchiveLibrary?
    /// Whether the Add panel is showing: the ways to add, and the intake queue.
    var showingAdd = false
    /// Counts changes of view, so what reads the remembered views follows them.
    private(set) var viewsChanged = 0
    /// What to do once the Add panel has closed: the sheet of the way to add chosen in it.
    var afterAdd: (() -> Void)?

    // Previews and the grid (#256).
    private(set) var previews: PreviewStore?
    /// Browse's list or grid, kept for every window.
    var browseLayout: BrowseLayout {
        didSet { defaults.set(browseLayout.rawValue, forKey: "browseLayout") }
    }
    private let defaults: UserDefaults

    var screen: Screen? = .library {
        didSet { if screen != oldValue { selectedItemID = nil } }
    }
    var selectedItemID: ItemSummary.ID? {
        didSet {
            if selectedItemID != oldValue {
                editing = nil
                recordOpened()
            }
        }
    }

    init(url: URL, options: OpenOptions? = nil, defaults: UserDefaults = .app) {
        self.url = url
        self.options = options
        self.defaults = defaults
        browseLayout =
            BrowseLayout(rawValue: defaults.string(forKey: "browseLayout") ?? "") ?? .list
        name = url.lastPathComponent
    }

    var root: URL { URL(filePath: handle?.root() ?? url.path(percentEncoded: false)) }

    /// Opens the archive and reads its collections and items, off the main thread.
    func load() async {
        isLoading = true
        defer { isLoading = false }
        let url = url
        let options = options
        do {
            let loaded = try await Task.detached {
                let handle =
                    if let options {
                        try ArchiveHandle.open(url, options: options)
                    } else {
                        try ArchiveHandle.open(url)
                    }
                return (handle, try handle.collections(), try handle.items())
            }.value
            handle = loaded.0
            name = loaded.0.name()
            isReadOnly = loaded.0.isReadOnly()
            collections = loaded.1
            tree = CollectionNode.tree(loaded.1)
            items = loaded.2
            problem = nil
            let archive = ArchiveModel(handle: loaded.0)
            self.archive = archive
            review = ReviewModel(archive: archive)
            review?.filed = { [weak self] in await self?.refreshItems() }
            questions = QuestionsModel(archive: archive)
            questions?.answerItem = { [weak self] question, answer in
                self?.answerItemQuestion(question, with: answer)
            }
            backups = BackupsModel(archive: archive)
            let crawler = CrawlerModel(archive: archive)
            crawler.didChange = { [weak self] in await self?.refreshItems() }
            self.crawler = crawler
            review?.showCrawler = { [weak crawler] in crawler?.toggle() }
            restructure = RestructureModel(archive: archive)
            restructure?.didChange = { [weak self] in await self?.refreshItems() }
            activeScreen = ActiveScreenModel(archive: archive, checkoutFolder: checkoutFolder) {
                [weak self] in
                await self?.refreshItems()
            }
            historyScreen = HistoryScreenModel(archive: archive) { [weak self] in
                await self?.refreshItems()
            }
            search = SearchModel(archive: archive)
            intake = IntakeModel(archive: archive)
            let browse = BrowseActionsModel(archive: archive) { [weak self] in
                await self?.refreshItems()
            }
            browse.created = { [weak self] path in self?.screen = .collection(path) }
            self.browse = browse
            librarian = LibrarianModel(archive: archive)
            librarian?.didChange = { [weak self] in await self?.refreshItems() }
            discover = DiscoverModel(archive: archive)
            let previews = PreviewStore(handle: loaded.0)
            self.previews = previews
            await previews.reload()
            await reloadLibrary()
            try? await archive.refreshAll()
            let devices = DevicesModel(archive: archive)
            self.devices = devices
            // Opened with options, as tests open the sample in place, it writes nothing.
            let writesProfile = !isReadOnly && options == nil
            Task { await devices.open(writingProfile: writesProfile) }
        } catch {
            problem = message(for: error)
        }
    }

    /// Where a checkout goes unless changed: the Desktop of the home the archive was opened
    /// with, as the core checks out by default.
    private var checkoutFolder: URL {
        #if os(macOS)
        options?.home.map { URL(filePath: $0).appending(path: "Desktop") }
            ?? FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask)[0]
        #else
        // Carrel's folder in Files, which has no Desktop.
        URL.documentsDirectory.appending(path: "Checked Out", directoryHint: .isDirectory)
        #endif
    }

    /// Reads the collections and items again, after a change that files or refiles items.
    func refreshItems() async {
        guard let handle else { return }
        guard
            let loaded = try? await Task.detached(operation: {
                (try handle.collections(), try handle.items())
            }).value
        else { return }
        collections = loaded.0
        tree = CollectionNode.tree(loaded.0)
        items = loaded.1
        await previews?.reload()
        await reloadLibrary()
    }

    /// How many items the Library lists as recently added.
    static let recentlyAdded: UInt32 = 12

    /// Reads the Library's shelves and what's new again, off the main thread.
    func reloadLibrary() async {
        guard let handle else { return }
        let recent = Self.recentlyAdded
        library = try? await Task.detached { try handle.library(recent: recent) }.value
    }

    /// What waits in each queue, for the Librarian section and the librarian's line.
    var waitingCounts: WaitingCounts {
        WaitingCounts(
            review: UInt32((archive?.reviewCount ?? 0) + (archive?.crawlerFindings.count ?? 0)),
            questions: UInt32(archive?.questions.count ?? 0),
            restructure: UInt32(archive?.restructureProposals.count ?? 0),
            flagged: UInt32(flaggedCollections.count + flaggedItems.count))
    }

    /// The Librarian section's rows: only the queues with something, in order, and none when
    /// nothing waits. A screen that's showing keeps its row, so deciding the last proposal
    /// doesn't pull the row out from under the owner.
    var waitingRows: [WaitingRow] {
        var rows = CarrelKit.waitingRows(counts: waitingCounts)
        if let showing = screen.flatMap(WaitingQueue.init(screen:)),
            !rows.contains(where: { $0.queue == showing })
        {
            rows.append(WaitingRow(queue: showing, count: 0))
            rows.sort { $0.queue.order < $1.queue.order }
        }
        return rows
    }

    /// Whether the window shows the Library as a page of its own, as wide as the window: on
    /// the Library, unless a search's results are showing.
    var showsLibraryPage: Bool {
        _ = viewsChanged
        return (screen == .library || screen == nil) && view(of: nil) == .covers
            && search?.isActive != true
    }

    /// Whether Checked Out shows in the sidebar: when anything is checked out or on the desk,
    /// or while it's showing.
    var showsCheckedOut: Bool {
        !(archive?.active.isEmpty ?? true) || screen == .active
    }

    /// The collection an item's card is filed in: the deepest collection folder holding it.
    func collectionPath(of item: ItemSummary) -> String? {
        let folder = (item.card as NSString).deletingLastPathComponent
        return collections.map(\.path)
            .filter { folder == $0 || folder.hasPrefix($0 + "/") }
            .max { $0.count < $1.count }
    }

    /// The items filed directly in the collection at `path`, by title.
    func items(in path: String) -> [ItemSummary] {
        items.filter { collectionPath(of: $0) == path }
            .sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
    }

    /// The content column's list for the chosen screen.
    var listedItems: [ItemSummary] {
        switch screen {
        case .collection(let path): items(in: path)
        case .library where view(of: nil) == .items:
            items.sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        default: []
        }
    }

    // MARK: Views (#512)

    static let viewsKey = "archiveViews"

    /// The collection the chosen screen's views look at: `nil` for the Library, a
    /// collection's path, or `nil` too for a screen that has no views.
    var viewScope: String? {
        if case .collection(let path) = screen { path } else { nil }
    }

    /// Whether the chosen screen has views: the Library and each collection.
    var hasViews: Bool {
        switch screen {
        case .library, .collection, nil: true
        default: false
        }
    }

    /// The view the Library (`nil`) or a collection shows: the last one chosen there, else
    /// its own, Covers or Items.
    func view(of scope: String?) -> ArchiveView {
        let views = archiveViews(collection: scope != nil)
        let remembered = (defaults.dictionary(forKey: Self.viewsKey)?[viewKey(scope)] as? String)
            .flatMap(archiveViewNamed(name:))
        return remembered.flatMap { views.contains($0) ? $0 : nil } ?? views[0]
    }

    /// Shows `view` of the Library or a collection, and remembers it for next time.
    func show(_ view: ArchiveView, of scope: String?) {
        var all = defaults.dictionary(forKey: Self.viewsKey) ?? [:]
        all[viewKey(scope)] = archiveViewName(view: view)
        defaults.set(all, forKey: Self.viewsKey)
        viewsChanged += 1
    }

    /// The view the chosen screen shows, for the toolbar's switcher.
    var currentView: ArchiveView {
        _ = viewsChanged
        return view(of: viewScope)
    }

    /// Where a scope's view is remembered: by the archive, then the collection.
    private func viewKey(_ scope: String?) -> String {
        "\(root.path(percentEncoded: false))|\(scope ?? "")"
    }

    /// Starts a wander from a random item in the Library or the chosen collection, and shows
    /// it.
    func wanderHere() {
        guard let discover else { return }
        discover.wander(within: viewScope)
        screen = .discover(.wander)
    }

    /// The chosen screen's name, shown under the archive's in the title bar.
    var screenTitle: String {
        switch screen {
        case .library, nil: "Library"
        case .status: "Status"
        case .review: WaitingQueue.review.title
        case .questions: WaitingQueue.questions.title
        case .restructure: WaitingQueue.restructure.title
        case .active: "Checked Out"
        case .flagged: WaitingQueue.flagged.title
        case .collection(let path): collections.first { $0.path == path }?.title ?? path
        case .history: "History"
        case .backups: "Backups"
        case .discover(let page): page.title
        }
    }

    var selectedItem: ItemSummary? {
        selectedItemID.flatMap { id in items.first { $0.id == id } }
    }

    var summary: ArchiveSummary {
        let kinds = Dictionary(grouping: items, by: \.kind).mapValues(\.count)
            .sorted { $0.value > $1.value || ($0.value == $1.value && "\($0.key)" < "\($1.key)") }
            .map { (kind: $0.key, count: $0.value) }
        return ArchiveSummary(
            itemCount: items.count, collectionCount: collections.count, kinds: kinds,
            holdingCount: items(in: "Holding").count)
    }

    /// The card of `item`, read from its file.
    func card(for item: ItemSummary) -> Card? {
        let url = root.appending(path: item.card)
        return (try? String(contentsOf: url, encoding: .utf8)).map(Card.init(text:))
    }
}

extension WaitingQueue {
    /// The queue `screen` decides, if it's one.
    init?(screen: Screen) {
        switch screen {
        case .review: self = .review
        case .questions: self = .questions
        case .restructure: self = .restructure
        case .flagged: self = .flagged
        default: return nil
        }
    }

    /// Its screen.
    var screen: Screen {
        switch self {
        case .review: .review
        case .questions: .questions
        case .restructure: .restructure
        case .flagged: .flagged
        }
    }

    /// Its place in the Librarian section.
    var order: Int {
        switch self {
        case .questions: 0
        case .review: 1
        case .restructure: 2
        case .flagged: 3
        }
    }

    /// Its name, from the core: Questions, To Approve, Suggestions or Flags.
    var title: String { waitingQueueTitle(queue: self) }

    var symbol: String {
        switch self {
        case .review: "tray.full"
        case .questions: "questionmark.bubble"
        case .restructure: "rectangle.3.group"
        case .flagged: "flag"
        }
    }
}
