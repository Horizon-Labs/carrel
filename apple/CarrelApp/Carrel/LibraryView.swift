// The Library (docs/design/library-home.md), where every archive opens: a line from the
// librarian when it's asking anything, the top-level collections as covers of their newest items, what was added
// most recently and On This Day, as `carrel library` and the interactive mode list them. An
// empty archive invites adding instead.

import CarrelKit
import SwiftUI

struct LibraryView: View {
    @Bindable var model: ArchiveWindowModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                if let line = waitingLine(counts: model.waitingCounts) {
                    LibrarianLine(
                        line: line, asks: librarianAsks(counts: model.waitingCounts)
                    ) { model.screen = $0.screen }
                }
                if let library = model.library {
                    if library.items == 0 {
                        EmptyLibrary(model: model)
                    } else {
                        Shelves(model: model, shelves: library.shelves)
                        if !library.recent.isEmpty {
                            RecentlyAdded(model: model, items: library.recent)
                        }
                    }
                }
                if let discover = model.discover, let day = discover.onThisDay,
                    !day.years.isEmpty
                {
                    OnThisDayCard(model: model, day: day)
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .overlay {
            if model.library == nil, model.isLoading { ProgressView() }
        }
        .task(id: model.discover.map(ObjectIdentifier.init)) {
            await model.discover?.refreshToday()
        }
        .viewSwitcher(model)
        .feedbackScreen(step: "library")
    }
}

/// What the librarian is asking, in its words, each part a link to its screen.
private struct LibrarianLine: View {
    static let scheme = "carrel-librarian"

    let line: String
    let asks: [LibrarianAsks]
    let open: (WaitingQueue) -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: "person.crop.circle.badge.questionmark")
                .foregroundStyle(.orange)
            Text(linked).font(.headline)
            Spacer()
        }
        .padding(12)
        .frame(maxWidth: .infinity)
        .background(.orange.opacity(0.15), in: .rect(cornerRadius: 10))
        .environment(
            \.openURL,
            OpenURLAction { url in
                guard url.scheme == Self.scheme,
                    let queue = asks.first(where: { "\($0.queue)" == url.host() })?.queue
                else { return .systemAction }
                open(queue)
                return .handled
            })
    }

    /// The line, with each part a link to its queue.
    private var linked: AttributedString {
        var text = AttributedString(line)
        for ask in asks {
            guard let range = text.range(of: ask.words),
                let url = URL(string: "\(Self.scheme)://\(ask.queue)")
            else { continue }
            text[range].link = url
        }
        return text
    }
}

/// An archive with nothing filed yet: what to do first.
private struct EmptyLibrary: View {
    let model: ArchiveWindowModel

    var body: some View {
        ContentUnavailableView {
            Label("Nothing Here Yet", systemImage: "books.vertical")
        } description: {
            Text("Drop files here or choose + to add.")
        } actions: {
            if !model.isReadOnly {
                Button("Add Items…", systemImage: "plus") { model.showingAdd = true }
            }
        }
        .frame(maxWidth: .infinity, minHeight: 320)
    }
}

/// The top-level collections, each a cover of its newest items.
private struct Shelves: View {
    let model: ArchiveWindowModel
    let shelves: [LibraryShelf]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Collections").font(.title2.bold())
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 200, maximum: 280), spacing: 16)],
                alignment: .leading, spacing: 20
            ) {
                ForEach(shelves, id: \.path) { shelf in
                    Button {
                        model.screen = .collection(shelf.path)
                    } label: {
                        ShelfCover(shelf: shelf, store: model.previews)
                    }
                    .buttonStyle(.plain)
                    .help(shelf.description ?? shelf.title)
                }
            }
        }
    }
}

private struct ShelfCover: View {
    let shelf: LibraryShelf
    let store: PreviewStore?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Grid(horizontalSpacing: 2, verticalSpacing: 2) {
                ForEach(0..<2, id: \.self) { row in
                    GridRow {
                        ForEach(0..<2, id: \.self) { column in
                            tile(row * 2 + column)
                        }
                    }
                }
            }
            .aspectRatio(4 / 3, contentMode: .fit)
            .clipShape(.rect(cornerRadius: 10))
            Text(shelf.title).font(.headline).lineLimit(1)
            if let description = shelf.description {
                Text(description)
                    .font(.callout).foregroundStyle(.secondary)
                    .lineLimit(2, reservesSpace: true)
            }
            Text(count).font(.caption).foregroundStyle(.secondary)
        }
        .contentShape(.rect)
    }

    private var count: String {
        var text = "\(shelf.items) item\(shelf.items == 1 ? "" : "s")"
        if shelf.collections > 0 {
            text += " · \(shelf.collections) collection\(shelf.collections == 1 ? "" : "s")"
        }
        return text
    }

    @ViewBuilder private func tile(_ index: Int) -> some View {
        let item = index < shelf.covers.count ? shelf.covers[index] : nil
        Rectangle()
            .fill(.quaternary)
            .overlay {
                if let item {
                    Thumbnail(id: item.id, kind: item.kind, store: store, iconSize: 22)
                } else if index == 0 {
                    Image(systemName: "folder")
                        .font(.system(size: 28, weight: .light))
                        .foregroundStyle(.secondary)
                }
            }
            .clipped()
    }
}

/// An item's picture from the preview store once it's made, else its kind's icon.
private struct Thumbnail: View {
    let id: String
    let kind: ItemKind
    let store: PreviewStore?
    var iconSize: CGFloat = 32

    var body: some View {
        Group {
            if let picture = store?.picture(for: id) {
                Image(decorative: picture, scale: 2).resizable().scaledToFill()
            } else {
                Image(systemName: kind.symbol)
                    .font(.system(size: iconSize, weight: .light))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: id) { await store?.loadThumbnail(id) }
    }
}

/// The items filed most recently, in a row.
private struct RecentlyAdded: View {
    let model: ArchiveWindowModel
    let items: [DiscoveredItem]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Recently Added").font(.title2.bold())
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: 14) {
                    ForEach(items, id: \.id) { item in
                        Button {
                            model.open(item: item.id)
                        } label: {
                            VStack(alignment: .leading, spacing: 4) {
                                Thumbnail(id: item.id, kind: item.kind, store: model.previews)
                                    .frame(width: 120, height: 120)
                                    .background(.quaternary, in: .rect(cornerRadius: 8))
                                    .clipShape(.rect(cornerRadius: 8))
                                Text(item.title).font(.caption).lineLimit(2, reservesSpace: true)
                                Text(item.collection)
                                    .font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                            }
                            .frame(width: 120)
                            .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                        .help(item.title)
                    }
                }
            }
            .scrollIndicators(.hidden)
        }
    }
}

/// Discover's On This Day, when anything happened on this day in other years.
private struct OnThisDayCard: View {
    let model: ArchiveWindowModel
    let day: OnThisDay

    /// "2 years ago · 2024".
    static func ago(_ year: DayInYear) -> String {
        "\(year.yearsAgo) year\(year.yearsAgo == 1 ? "" : "s") ago · \(String(year.year))"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("On This Day").font(.title2.bold())
            ForEach(day.years.prefix(3), id: \.year) { year in
                VStack(alignment: .leading, spacing: 4) {
                    Text(Self.ago(year))
                        .font(.subheadline).foregroundStyle(.secondary)
                    ForEach(year.items.prefix(4), id: \.item.id) { happened in
                        Button {
                            model.open(item: happened.item.id)
                        } label: {
                            Label(happened.item.title, systemImage: happened.item.kind.symbol)
                        }
                        .linkButtonStyle()
                    }
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.5), in: .rect(cornerRadius: 12))
    }
}
