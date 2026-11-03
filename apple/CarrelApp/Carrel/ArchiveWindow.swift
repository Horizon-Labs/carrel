// An archive's window: the screens in the sidebar, the chosen screen's list, and the chosen
// thing in the inspector column.

import CarrelKit
import SwiftUI

struct ArchiveWindow: View {
    static let windowID = "archive"

    @Environment(AppModel.self) private var app
    @Environment(\.dismissWindow) private var dismissWindow
    @Environment(\.openWindow) private var openWindow
    #if os(macOS)
    @Environment(\.openSettings) private var openSettings
    @Environment(\.controlActiveState) private var activeState
    #else
    @Environment(MobileLibrary.self) private var library
    @Environment(\.scenePhase) private var scenePhase
    #endif
    @State private var model: ArchiveWindowModel
    /// On iPhone, the chosen screen comes first rather than the sidebar: the Library when the
    /// archive opens.
    @State private var compactColumn = NavigationSplitViewColumn.content
    @State private var compactLibraryColumn = NavigationSplitViewColumn.detail

    init(url: URL) {
        _model = State(initialValue: ArchiveWindowModel(url: url))
    }

    var body: some View {
        @Bindable var model = model
        Group {
            // The Library is a page as wide as the window, as Photos' is; the other screens
            // have a list beside the inspector.
            if model.showsLibraryPage {
                NavigationSplitView(preferredCompactColumn: $compactLibraryColumn) {
                    sidebar
                } detail: {
                    ArchiveScreenContent(model: model)
                        #if os(iOS)
                    .feedbackToolbarItem(model)
                        #endif
                }
            } else {
                NavigationSplitView(preferredCompactColumn: $compactColumn) {
                    sidebar
                } content: {
                    ArchiveScreenContent(model: model)
                        .navigationSplitViewColumnWidth(min: 260, ideal: 340)
                        #if os(iOS)
                    .feedbackToolbarItem(model)
                        #endif
                } detail: {
                    Inspector(model: model)
                        #if os(iOS)
                    .feedbackToolbarItem(model)
                        #endif
                }
            }
        }
        .feedbackActions(model)
        .activeAndHistorySheets(model)
        .modifier(ArchiveWindowActions(model: model))
        .browseSheets(model.browse)
        .librarianPanel(model)
        .addPanel(model)
        #if os(macOS)
        .toolbarSearchField(model)
        #endif
        .navigationTitle(model.name)
        #if os(macOS)
        .navigationSubtitle(
            model.isReadOnly ? "\(model.screenTitle) · Read only" : model.screenTitle
        )
        #endif
        .overlay {
            if let problem = model.problem {
                ContentUnavailableView {
                    Label("Can’t Open “\(model.url.lastPathComponent)”", systemImage: "archivebox")
                } description: {
                    Text(problem)
                } actions: {
                    Button("Try Again") { Task { await model.load() } }
                }
                .background(.background)
            } else if model.handle == nil {
                // Until the archive is read, which takes a while when iCloud downloads it.
                ProgressView("Opening “\(model.url.lastPathComponent)”…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(.background)
            }
        }
        .sheet(item: $model.correction) { CorrectionSheet(correction: $0) }
        .sheet(item: $model.missingFeature) { MissingFeatureSheet(window: model, model: $0) }
        .icloudMoveSheet(model)
        .archiveErrorAlert(model)
        .focusedSceneValue(model)
        .task {
            #if os(macOS)
            dismissWindow(id: WelcomeView.windowID)
            #endif
            await model.load()
            choosePendingItem()
            #if os(macOS)
            await model.followOutsideChanges()
            #endif
        }
        #if os(iOS)
        // What changed on other devices arrives through iCloud while the app is away.
        .onChange(of: scenePhase) {
            if scenePhase == .active { Task { await model.reloadAll() } }
        }
        #endif
        .onChange(of: app.pendingLink) { choosePendingItem() }
        .onChange(of: app.pendingScreen, initial: true) {
            if let screen = app.takePendingScreen(for: model.root) { model.screen = screen }
        }
        #if os(macOS)
        .handlesExternalEvents(preferring: [AgentLink.scheme], allowing: [AgentLink.scheme])
        .onOpenURL { url in
            app.open(link: url) { openWindow(id: Self.windowID, value: $0) }
        }
        .onChange(of: activeState, initial: true) {
            if activeState == .key { app.activeArchive = model.root }
        }
        #else
        .onAppear { app.activeArchive = model.root }
        #endif
    }
}

extension ArchiveWindow {
    /// The sidebar, with its toolbar.
    fileprivate var sidebar: some View {
        ArchiveSidebar(model: model)
            .navigationSplitViewColumnWidth(min: 180, ideal: 220)
            .feedbackToolbarItem(model)
            #if os(iOS)
        .mobileArchiveToolbar(model)
            #endif
    }

    /// Chooses the item a link asked for: on Active when it's checked out or on the desk, as
    /// in the agent's menu, or when a link from Finder asks to do something with it, which
    /// starts; else in Browse, as from Spotlight.
    /// A screen a link asks for is shown first, so an item it names is chosen where it is.
    fileprivate func choosePendingItem() {
        guard model.handle != nil, let link = app.takePendingLink(for: model.root) else { return }
        if let screen = link.screen {
            model.show(linked: screen)
            #if os(macOS)
            if screen == .settings { openSettings() }
            #else
            if screen == .settings { library.showingSettings = true }
            #endif
        }
        guard let item = link.item else { return }
        if link.action == nil, model.archive?.active.contains(where: { $0.id == item }) == false {
            model.open(item: item)
            return
        }
        model.screen = .active
        model.selectedItemID = item
        if let action = link.action { Task { await model.start(action, on: item) } }
    }
}

/// The sidebar, in the order of what owners do most (docs/design/library-home.md): the
/// Library, Checked Out, what the Librarian is asking, the collections, then looking after the
/// archive. Checked Out and the Librarian's rows show only when they have something.
/// Discover's pages are views of the Library and of each collection, in the toolbar.
struct ArchiveSidebar: View {
    @Bindable var model: ArchiveWindowModel

    var body: some View {
        // A row's tag goes last, after its badge: a tag with a modifier after it isn't the
        // row's, so the row can't be chosen (#334).
        List(selection: $model.screen) {
            Section {
                Label("Library", systemImage: "books.vertical").tag(Screen.library)
                if model.showsCheckedOut {
                    Label("Checked Out", systemImage: "arrow.up.forward.square")
                        .badge(model.archive?.active.count ?? 0)
                        .tag(Screen.active)
                }
            }
            let waiting = model.waitingRows
            if !waiting.isEmpty {
                Section("Librarian") {
                    ForEach(waiting, id: \.queue) { row in
                        Label(row.queue.title, systemImage: row.queue.symbol)
                            .badge(Int(row.count))
                            .tag(row.queue.screen)
                    }
                }
                .tint(.orange)
            }
            Section("Collections") {
                OutlineGroup(model.tree, children: \.children) { node in
                    Label {
                        HStack {
                            Text(node.collection.title)
                            FlagMark(flag: node.collection.flag)
                        }
                    } icon: {
                        Image(systemName: "folder")
                    }
                    .badge(model.items(in: node.collection.path).count)
                    .help(node.collection.description ?? node.collection.path)
                    .contextMenu {
                        CollectionFlagButtons(model: model, collection: node.collection)
                        AskAboutCollectionButton(model: model, collection: node.collection)
                    }
                    .tag(Screen.collection(node.collection.path))
                }
            }
            Section("Archive") {
                Label("Status", systemImage: "gauge.with.dots.needle.33percent").tag(Screen.status)
                Label("History", systemImage: "clock.arrow.circlepath").tag(Screen.history)
                Label("Backups", systemImage: "externaldrive.badge.timemachine").tag(Screen.backups)
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom, spacing: 0) { BuildBadge() }
    }
}

/// The content column for the chosen screen.
struct ArchiveScreenContent: View {
    @Bindable var model: ArchiveWindowModel

    var body: some View {
        if let search = model.search, search.isActive {
            SearchResultsView(model: model, search: search)
        } else {
            screen
        }
    }

    /// The items of the Library or the collection, as a list or a grid, with Browse's layout
    /// picker.
    private var itemsView: some View {
        Group {
            if model.browseLayout == .grid {
                BrowseGrid(model: model)
            } else {
                itemsList
            }
        }
        .overlay {
            if model.listedItems.isEmpty, !model.isLoading {
                ContentUnavailableView("No Items", systemImage: "folder")
            }
        }
        .toolbar {
            Picker("Layout", selection: $model.browseLayout) {
                Label("List", systemImage: "list.bullet").tag(BrowseLayout.list)
                Label("Grid", systemImage: "square.grid.2x2").tag(BrowseLayout.grid)
            }
            .pickerStyle(.segmented)
            .help("Show the items as a list or a grid")
        }
    }

    private var itemsList: some View {
        List(model.listedItems, selection: $model.selectedItemID) { item in
            HStack {
                Label(item.title, systemImage: item.kind.symbol)
                Spacer()
                FlagMark(flag: item.flag)
            }
        }
        .contextMenu(forSelectionType: ItemSummary.ID.self) { ids in
            BrowseActions(model: model, ids: ids)
        }
    }

    /// A view other than the scope's own, Discover's page before, looking only at the
    /// collection chosen and those inside it, or at the whole archive in the Library.
    @ViewBuilder private var scopedView: some View {
        if let discover = model.discover, let page = model.currentView.page {
            DiscoverView(page: page, discover: discover, window: model, scope: model.viewScope)
                .viewSwitcher(model)
        }
    }

    @ViewBuilder private var screen: some View {
        switch model.screen {
        case .library, nil:
            switch model.currentView {
            case .covers: LibraryView(model: model)
            case .items: itemsList.viewSwitcher(model)
            default: scopedView
            }
        case .status:
            StatusView(model: model)
        case .collection:
            if model.currentView == .items {
                itemsView.viewSwitcher(model)
            } else {
                scopedView
            }
        case .review:
            VStack(spacing: 0) {
                if let crawler = model.crawler {
                    ReviewSourcePicker(
                        crawler: crawler, proposals: model.review?.proposals.count ?? 0
                    )
                    .padding(8)
                }
                if let crawler = model.crawler, crawler.isShown {
                    CrawlerFindingsView(crawler: crawler)
                } else if let review = model.review {
                    ReviewView(review: review, collections: model.collections.map(\.path))
                }
            }
        case .questions:
            if let questions = model.questions { QuestionsView(questions: questions) }
        case .restructure:
            if let restructure = model.restructure {
                RestructureView(
                    restructure: restructure, collections: model.collections.map(\.path))
            }
        case .active:
            if let active = model.activeScreen { ActiveView(model: active) }
        case .flagged:
            FlaggedView(model: model)
        case .history:
            if let history = model.historyScreen { HistoryView(model: history) }
        case .backups:
            #if os(macOS)
            if let backups = model.backups { BackupsView(backups: backups) }
            #else
            ContentUnavailableView(
                "Backups Are Made on a Mac", systemImage: "externaldrive.badge.timemachine",
                description: Text(
                    "Kopia, which takes the archive’s snapshots, runs only on a Mac. What’s replaced here is kept in the archive’s holding folder until a Mac snapshots it."
                ))
            #endif
        case .discover(let page):
            if let discover = model.discover {
                DiscoverView(page: page, discover: discover, window: model)
            }
        }
    }
}

/// The inspector column: the chosen item's preview and card.
private struct Inspector: View {
    let model: ArchiveWindowModel

    var body: some View {
        if model.screen == .review, let crawler = model.crawler, crawler.isShown {
            if let finding = crawler.inspected {
                CrawlerFindingInspector(finding: finding, crawler: crawler, window: model)
                    .feedbackScreen(step: "crawler finding")
            } else {
                ContentUnavailableView("Nothing Selected", systemImage: "binoculars")
            }
        } else if model.screen == .review, let review = model.review,
            let proposal = review.inspected
        {
            ProposalInspector(proposal: proposal, review: review, correct: model.correct) {
                model.pickReading($0, of: proposal)
            }
            .feedbackScreen(step: "proposal inspector")
        } else if model.screen == .questions, let questions = model.questions,
            let question = questions.selected
        {
            QuestionInspector(
                question: question, questions: questions,
                collections: model.collections.map(\.path), previews: model.previews
            ) { id in model.items.first { $0.id == id } }
        } else if model.screen == .restructure, let restructure = model.restructure,
            let proposal = restructure.selected
        {
            RestructureInspector(proposal: proposal, restructure: restructure)
                .feedbackScreen(step: "restructure proposal")
        } else if model.screen == .active, let active = model.activeScreen {
            ActiveInspector(model: active)
        } else if model.screen == .history, let history = model.historyScreen {
            ChangeInspector(model: history) { id in model.items.first { $0.id == id }?.title }
        } else if isMac, model.screen == .backups, let backups = model.backups,
            let change = backups.selected
        {
            #if os(macOS)
            BackupInspector(change: change, backups: backups, items: model.items)
            #endif
        } else if let item = model.selectedItem {
            if let edit = model.editing, edit.target == .item(item.id) {
                ItemEditor(item: item, edit: edit, items: model.items) { model.editing = nil }
                    .feedbackScreen(step: "editing the card")
            } else {
                VStack(spacing: 0) {
                    if let previews = model.previews {
                        ItemPreviewView(item: item, store: previews)
                    }
                    ItemCardView(
                        item: item, card: model.card(for: item), root: model.root,
                        archive: model.archive, open: model.open(item:)
                    ) {
                        ItemActions(model: model, item: item)
                    } below: {
                        ItemFlagSection(item: item, window: model)
                        ItemQuestionsSection(item: item, window: model)
                    }
                }
                .id(item.id)
            }
        } else if case .collection(let path) = model.screen,
            let collection = model.collections.first(where: { $0.path == path })
        {
            CollectionInspector(collection: collection, window: model)
                .feedbackScreen(step: "collection inspector")
        } else {
            ContentUnavailableView("Nothing Selected", systemImage: "sidebar.right")
        }
    }
}

extension ItemKind {
    var symbol: String {
        switch self {
        case .file: "doc"
        case .folder: "folder"
        case .bundle: "shippingbox"
        case .physical: "books.vertical"
        case .url: "link"
        case .external: "externaldrive"
        case .tombstone: "archivebox"
        }
    }

    var name: String {
        switch self {
        case .file: "File"
        case .folder: "Folder"
        case .bundle: "Bundle"
        case .physical: "Physical"
        case .url: "Web page"
        case .external: "External"
        case .tombstone: "Removed"
        }
    }
}
