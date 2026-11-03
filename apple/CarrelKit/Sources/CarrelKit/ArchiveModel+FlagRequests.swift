// Flags as requests to the librarian (#454), as `carrel flagged` offers them: each flag with
// where the librarian is with it, handing one over now, answering what it asked, accepting its
// fix (which clears the flag), and asking it to look again. Each runs the librarian on `model`
// or this device's strongest that fits in `memoryGB`.

extension ArchiveModel {
    /// Everything flagged, each with where the librarian is with it.
    public func flagRequests() async throws -> [FlagRequestView] {
        try await call { try $0.flagRequests() }
    }

    /// The flag on `target` (an item's identifier or a collection's folder) and where the
    /// librarian is with it.
    public func flagRequest(_ target: String) async throws -> FlagRequestView {
        try await call { try $0.flagRequest(target: target) }
    }

    /// Hands the flag on `target` to the librarian now.
    @discardableResult
    public func handFlagToLibrarian(
        _ target: String, model: ModelProvider? = nil, memoryGB: UInt64?
    ) async throws -> FlagRequestView {
        try await afterLibrarian(
            try await call(reporting: "The librarian is looking at it") { handle, _ in
                try handle.handFlagToLibrarian(target: target, model: model, memoryGb: memoryGB)
            })
    }

    /// Answers what the librarian asked about the flag on `target`; it tries again at once.
    @discardableResult
    public func answerFlag(
        _ target: String, answer: String, model: ModelProvider? = nil, memoryGB: UInt64?
    ) async throws -> FlagRequestView {
        try await afterLibrarian(
            try await call(reporting: "The librarian is trying again") { handle, _ in
                try handle.answerFlag(
                    target: target, answer: answer, model: model, memoryGb: memoryGB)
            })
    }

    /// Answers the flag question `id` from the queue; the librarian tries again at once.
    @discardableResult
    public func answerFlagQuestion(
        _ id: String, answer: String, model: ModelProvider? = nil, memoryGB: UInt64?
    ) async throws -> FlagRequestView {
        try await afterLibrarian(
            try await call(reporting: "The librarian is trying again") { handle, _ in
                try handle.answerFlagQuestion(
                    id: id, answer: answer, model: model, memoryGb: memoryGB)
            })
    }

    /// Asks the librarian to look at the flag on `target` again, now.
    @discardableResult
    public func lookAtFlagAgain(
        _ target: String, model: ModelProvider? = nil, memoryGB: UInt64?
    ) async throws -> FlagRequestView {
        try await afterLibrarian(
            try await call(reporting: "The librarian is looking again") { handle, _ in
                try handle.lookAtFlagAgain(target: target, model: model, memoryGb: memoryGB)
            })
    }

    /// Accepts fix `number` (from 1) for the flag on `target`: the change is made as the
    /// owner's, and the flag is cleared once no other fix waits.
    @discardableResult
    public func acceptFlagFix(_ target: String, number: UInt32) async throws -> FlagFixAccepted {
        let accepted = try await call { try $0.acceptFlagFix(target: target, number: number) }
        try await refreshStatus()
        if status?.hasHistory == true { try await refreshHistory() }
        return accepted
    }

    /// The queue and Status follow what the librarian did: a question, or a change it made.
    private func afterLibrarian(_ view: FlagRequestView) async throws -> FlagRequestView {
        try await refreshQuestions()
        try await refreshStatus()
        if status?.hasHistory == true { try await refreshHistory() }
        return view
    }
}
