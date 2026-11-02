// Where the owner is, for feedback (#487): the screen, the step of its workflow and what's in
// view. The window works out a base from its screen and selection, so every screen has one
// for free; a view says more with `.feedbackScreen(step:inView:)`, such as Review's proposal
// inspector or Browse's collection inspector. Identifiers and titles only, never contents.

import CarrelKit
import Foundation
import Observation
import SwiftUI

/// What a view declares about where the owner is.
struct FeedbackMark: Equatable {
    /// The screen, when the view names one of its own (a sheet or panel); `nil` keeps the
    /// window's.
    var screen: String?
    /// The step in its workflow, such as `collection inspector`.
    var step: String?
    /// What's in view there.
    var inView: [FeedbackInView] = []
}

/// The marks of the views showing in a window, latest shown last.
@Observable
final class FeedbackMarks {
    private var marks: [UUID: (order: Int, mark: FeedbackMark)] = [:]
    private var next = 0

    func set(_ mark: FeedbackMark, for id: UUID) {
        let order = marks[id]?.order ?? next
        if marks[id] == nil { next += 1 }
        marks[id] = (order, mark)
    }

    func remove(_ id: UUID) {
        marks[id] = nil
    }

    /// `base` with the marks laid over it: a later mark's screen and step win, and what's in
    /// view adds up.
    func applied(to base: FeedbackScreen) -> FeedbackScreen {
        var screen = base
        for (_, mark) in marks.values.sorted(by: { $0.order < $1.order }) {
            if let name = mark.screen { screen.screen = name }
            if let step = mark.step { screen.step = step }
            for view in mark.inView where !screen.inView.contains(view) {
                screen.inView.append(view)
            }
        }
        return screen
    }
}

private struct FeedbackMarking: ViewModifier {
    let mark: FeedbackMark
    @Environment(FeedbackMarks.self) private var marks: FeedbackMarks?
    @State private var id = UUID()

    func body(content: Content) -> some View {
        content
            .onAppear { marks?.set(mark, for: id) }
            .onChange(of: mark) { marks?.set(mark, for: id) }
            .onDisappear { marks?.remove(id) }
    }
}

extension View {
    /// Declares, for feedback, the step of the workflow this view shows and what's in view;
    /// `screen` names a screen of its own, as a sheet or panel does.
    func feedbackScreen(
        _ screen: String? = nil, step: String? = nil, inView: [FeedbackInView] = []
    ) -> some View {
        modifier(FeedbackMarking(mark: FeedbackMark(screen: screen, step: step, inView: inView)))
    }
}

extension FeedbackInView {
    static func item(_ item: ItemSummary) -> Self {
        FeedbackInView(kind: "item", id: item.id, title: item.title)
    }

    static func collection(_ collection: CollectionSummary) -> Self {
        FeedbackInView(kind: "collection", id: collection.path, title: collection.title)
    }
}

extension ArchiveWindowModel {
    /// Where the owner is, from the screen and what's chosen in it, before the views' marks.
    var feedbackBase: FeedbackScreen {
        var screen = FeedbackScreen(screen: screenTitle, step: "", inView: [])
        if let search, search.isActive {
            screen.screen = "Search"
            screen.step = "results"
        } else {
            switch self.screen {
            case .collection(let path):
                screen.screen = "Browse"
                screen.step = selectedItem == nil ? "collection" : "item"
                if let collection = collections.first(where: { $0.path == path }) {
                    screen.inView.append(.collection(collection))
                }
            case .review:
                if let proposal = review?.inspected {
                    screen.step = "proposal"
                    screen.inView.append(
                        FeedbackInView(kind: "proposal", id: proposal.id, title: ""))
                }
            case .questions:
                if let question = questions?.selected {
                    screen.step = "question"
                    screen.inView.append(
                        FeedbackInView(
                            kind: "question", id: question.id, title: question.subject))
                }
            case .discover:
                screen.step = screen.screen
                screen.screen = "Discover"
            default:
                break
            }
        }
        if let item = selectedItem { screen.inView.append(.item(item)) }
        return screen
    }

    /// Opens the feedback form about where the owner is now, with the window's screenshot
    /// taken first, so the form isn't in it.
    func startFeedback() {
        guard archive != nil, feedback == nil else { return }
        feedback = FeedbackModel(
            screen: feedbackMarks.applied(to: feedbackBase),
            screenshot: FeedbackModel.windowScreenshot())
    }
}
