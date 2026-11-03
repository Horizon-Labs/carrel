// The archive window's sidebar (#334, #512): every screen in it can be chosen, by selecting its
// row as a click does, and the window then has that screen's content to show. The rows follow
// docs/design/library-home.md's order, and Checked Out and Waiting's rows show only when they
// have something. Each test runs against its own temporary copy of the sample archive, once
// with proposals and questions waiting and once with both queues empty.

import AppKit
import CarrelKit
import SwiftUI
import XCTest

@testable import Carrel

@MainActor
final class SidebarTests: XCTestCase {
    func testEveryScreenCanBeChosenWithItemsWaiting() async throws {
        let sample = try await SampleWindow(questions: [
            QuestionsModelTests.term: question(
                QuestionsModelTests.term, kind: "term", subject: "PLA")
        ])
        defer { sample.remove() }
        XCTAssertEqual(sample.archive.reviewCount, 1)
        XCTAssertEqual(sample.archive.questions.count, 1)
        let chosen = try await checkEveryScreen(sample.model)
        XCTAssert(chosen.contains(.review) && chosen.contains(.questions), "\(chosen)")
    }

    func testEveryScreenCanBeChosenWithEmptyQueues() async throws {
        let sample = try await SampleWindow()
        defer { sample.remove() }
        let review = sample.root.appending(path: "_System/queue/review")
        for entry in try FileManager.default.contentsOfDirectory(atPath: review.path()) {
            try FileManager.default.removeItem(at: review.appending(path: entry))
        }
        try await sample.archive.refreshAll()
        XCTAssertEqual(sample.archive.reviewCount, 0)
        XCTAssertEqual(sample.archive.questions.count, 0)
        let chosen = try await checkEveryScreen(sample.model)
        XCTAssertFalse(chosen.contains(.review), "an empty queue has no row: \(chosen)")
        XCTAssertFalse(chosen.contains(.questions), "\(chosen)")
    }

    func testTheWindowOpensOnTheLibrary() async throws {
        let sample = try await SampleWindow()
        defer { sample.remove() }
        let model = sample.model
        XCTAssertEqual(model.screen, .library)
        XCTAssertEqual(model.screenTitle, "Library")
        let library = try XCTUnwrap(model.library)
        XCTAssertEqual(library.shelves.map(\.path), ["Projects", "Reading", "Holding"])
        XCTAssertFalse(library.recent.isEmpty)
        XCTAssertEqual(waitingLine(counts: model.waitingCounts), "1 to review")
        model.show(linked: .library)
        XCTAssertEqual(model.screen, .library)
        model.show(linked: .ingest)
        XCTAssertTrue(model.showingAdd, "adding is the Add panel, not a screen")
    }

    func testAShowingQueueKeepsItsRowAndDiscoverOpensWhenAsked() async throws {
        let sample = try await SampleWindow()
        defer { sample.remove() }
        let model = sample.model
        XCTAssertEqual(model.archive?.restructureProposals.count, 0)
        XCTAssertFalse(model.waitingRows.contains { $0.queue == .restructure })
        model.screen = .restructure
        XCTAssertEqual(
            model.waitingRows.map(\.queue), [.review, .restructure],
            "the row stays while its screen shows")

        // Discover is collapsed until opened, and remembers being opened.
        let key = ArchiveSidebar.discoverExpandedKey
        defer { UserDefaults.app.removeObject(forKey: key) }
        UserDefaults.app.removeObject(forKey: key)
        model.screen = nil
        let collapsed = try rows(of: model)
        UserDefaults.app.set(true, forKey: key)
        let expanded = try rows(of: model)
        XCTAssertEqual(expanded - collapsed, DiscoverPage.allCases.count)
    }

    /// How many rows the sidebar shows.
    private func rows(of model: ArchiveWindowModel) throws -> Int {
        let sidebar = Hosted(ArchiveSidebar(model: model))
        return try XCTUnwrap(sidebar.view.firstDescendant(NSOutlineView.self)).numberOfRows
    }

    /// Selects each of the sidebar's rows in turn and checks the window follows to its screen,
    /// and that the content column shows something for it. Discover is opened, so its pages
    /// are among them. Returns the screens chosen, in order.
    @discardableResult
    private func checkEveryScreen(_ model: ArchiveWindowModel) async throws -> [Screen] {
        let key = ArchiveSidebar.discoverExpandedKey
        UserDefaults.app.set(true, forKey: key)
        defer { UserDefaults.app.removeObject(forKey: key) }
        model.screen = nil
        let waiting = model.waitingRows.map(\.queue.screen)
        let checkedOut: [Screen] = model.showsCheckedOut ? [.active] : []
        let sidebar = Hosted(ArchiveSidebar(model: model))
        let outline = try XCTUnwrap(sidebar.view.firstDescendant(NSOutlineView.self))
        var chosen: [Screen] = []
        for row in 0..<outline.numberOfRows {
            model.screen = nil
            outline.selectRowIndexes([row], byExtendingSelection: false)
            await settle()
            guard let screen = model.screen else { continue }
            chosen.append(screen)
            XCTAssertEqual(model.screenTitle, title(of: screen, in: model))
            XCTAssert(hasContent(screen, model), "\(screen) has nothing to show")
            // The content column draws it.
            _ = Hosted(ArchiveScreenContent(model: model))
            await settle()
        }
        let expected: [Screen] =
            [.library] + checkedOut + waiting
            + model.tree.map { .collection($0.collection.path) }
            + DiscoverPage.allCases.map { .discover($0) } + [.status, .history, .backups]
        XCTAssertEqual(chosen, expected)
        return chosen
    }

    /// Whether the window has what the content column draws for `screen`: the screen's own
    /// model, or for Status and Browse the open archive. Without it the column is empty.
    private func hasContent(_ screen: Screen, _ model: ArchiveWindowModel) -> Bool {
        switch screen {
        case .library: model.library != nil
        case .status, .collection, .flagged: model.handle != nil
        case .review: model.review != nil
        case .questions: model.questions != nil
        case .restructure: model.restructure != nil
        case .active: model.activeScreen != nil
        case .history: model.historyScreen != nil
        case .backups: model.backups != nil
        case .discover: model.discover != nil
        }
    }

    private func title(of screen: Screen, in model: ArchiveWindowModel) -> String {
        let before = model.screen
        defer { model.screen = before }
        model.screen = screen
        return model.screenTitle
    }

    /// Lets SwiftUI and AppKit catch up with a change.
    private func settle() async {
        for _ in 0..<3 { try? await Task.sleep(for: .milliseconds(50)) }
    }
}

/// A SwiftUI view in a window of its own, laid out, as the app shows it.
@MainActor
private struct Hosted<Content: View> {
    let window: NSWindow
    let view: NSHostingView<Content>

    init(_ content: Content) {
        view = NSHostingView(rootView: content)
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 1200), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = view
        view.layoutSubtreeIfNeeded()
    }

}

extension NSView {
    /// The first view of type `type` in this one, searching depth first.
    fileprivate func firstDescendant<V: NSView>(_ type: V.Type) -> V? {
        for subview in subviews {
            if let match = subview as? V ?? subview.firstDescendant(type) { return match }
        }
        return nil
    }

}
