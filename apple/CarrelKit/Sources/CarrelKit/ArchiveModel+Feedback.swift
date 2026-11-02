// Feedback from any screen (#487): the owner's kind and words, the screen and step the view
// declares, and the window's screenshot when kept, saved in the archive's
// `_System/feedback/<device-id>/` without the writer lease. Nothing is posted: an entry's issue
// draft opens in the browser for the owner to submit.

extension ArchiveModel {
    /// Saves the owner's feedback from this device.
    @discardableResult
    public func sendFeedback(_ sending: FeedbackSending) async throws -> FeedbackEntry {
        try await call { try $0.sendFeedback(sending: sending) }
    }

    /// Every device's feedback, oldest first.
    public func feedbackEntries() async throws -> [FeedbackEntry] {
        try await call { try $0.feedbackEntries() }
    }

    /// How many entries haven't been marked sent.
    public func unsentFeedback() async throws -> UInt32 {
        try await call { try $0.unsentFeedback() }
    }

    /// A Markdown bundle of the feedback, grouped by screen.
    public func feedbackExport(unsentOnly: Bool) async throws -> String {
        try await call { try $0.feedbackExport(unsentOnly: unsentOnly) }
    }

    /// The issue draft for the entry `name`, to open in the browser.
    public func feedbackIssueDraft(_ name: String) async throws -> FeedbackIssueDraft {
        try await call { try $0.feedbackIssueDraft(name: name) }
    }

    /// Marks the entry `name` sent, as the issue at `address` (none when empty).
    @discardableResult
    public func markFeedbackSent(_ name: String, at address: String) async throws -> FeedbackEntry {
        try await call { try $0.markFeedbackSent(name: name, address: address) }
    }
}
