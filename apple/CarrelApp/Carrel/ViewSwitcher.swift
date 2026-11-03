// The views of the Library and of each collection (docs/design/library-home.md): a switcher in
// the toolbar, as Finder's and Photos' view modes are, with Covers (the Library's own), Items
// (a collection's own), Timeline, Map, Connections and Activity, Discover's pages before. Each
// looks only at the collection and those inside it, or at the whole archive in the Library,
// and the last one is remembered for each. Wander starts from a random item there.

import CarrelKit
import SwiftUI

extension ArchiveView {
    /// The Discover page that draws it, for every view but Covers and Items.
    var page: DiscoverPage? {
        switch self {
        case .covers, .items: nil
        case .timeline: .timeline
        case .map: .map
        case .connections: .graph
        case .activity: .activity
        }
    }

    var title: String { archiveViewTitle(view: self) }

    var symbol: String {
        switch self {
        case .covers: "square.grid.2x2"
        case .items: "list.bullet"
        case .timeline: "chart.bar.xaxis"
        case .map: "square.grid.3x3.square"
        case .connections: "point.3.connected.trianglepath.dotted"
        case .activity: "square.grid.4x3.fill"
        }
    }
}

extension View {
    /// The view switcher and Wander, in the toolbar of the Library and each collection.
    func viewSwitcher(_ model: ArchiveWindowModel) -> some View {
        modifier(ViewSwitcher(model: model))
    }
}

private struct ViewSwitcher: ViewModifier {
    let model: ArchiveWindowModel

    func body(content: Content) -> some View {
        content.toolbar {
            if model.hasViews {
                ToolbarItemGroup {
                    Picker("View", selection: view) {
                        ForEach(archiveViews(collection: model.viewScope != nil), id: \.self) {
                            Label($0.title, systemImage: $0.symbol).tag($0)
                        }
                    }
                    .pickerStyle(.menu)
                    .help(
                        "How to look at \(model.viewScope == nil ? "the archive" : "this collection")"
                    )
                    Button("Wander", systemImage: "figure.walk") { model.wanderHere() }
                        .help("Walk through the stacks from a random item here")
                        .disabled(model.discover == nil)
                }
            }
        }
    }

    private var view: Binding<ArchiveView> {
        Binding(
            get: { model.currentView },
            set: { model.show($0, of: model.viewScope) })
    }
}
