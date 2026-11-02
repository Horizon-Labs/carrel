// Flags as requests to the librarian (#454), as `carrel flagged` and the interactive mode's
// Flags view offer them: under a flagged item's card and in a flagged collection's inspector,
// and on the Flagged screen, the librarian's question with a field to answer it, its fixes to
// accept (which clears the flag once none waits), why it can't help, with Report… when Carrel
// is missing something, and Ask the Librarian Now or Look Again. Flag questions in the queue
// are answered the same way, and the agent hands waiting flags over when the Mac is idle.

import CarrelKit
import SwiftUI

extension FlagState {
    /// The symbol beside where a flag's request stands.
    var symbol: String {
        switch self {
        case .waiting: "hourglass"
        case .asked: "questionmark.bubble"
        case .suggested: "wand.and.stars"
        case .cantHelp: "exclamationmark.bubble"
        case .done: "checkmark.circle"
        }
    }
}

extension ArchiveWindowModel {
    /// Answers the flag question `question` with `answer`: the librarian tries again at once.
    /// What came of it, in a sentence.
    func answerFlagQuestion(_ question: Question, with answer: String) async -> String? {
        guard let archive else { return nil }
        do {
            let view = try await archive.answerFlagQuestion(
                question.id, answer: answer, memoryGB: LibrarianModel.thisMac)
            await refreshFlags()
            return "The librarian tried again: \(view.stateText)."
        } catch {
            archive.lastError = nil
            return (error as? CarrelError)?.message ?? error.localizedDescription
        }
    }

    /// Reads the flags again, after the librarian or the owner changed one.
    func refreshFlags() async {
        flagRequests = (try? await archive?.flagRequests()) ?? flagRequests
    }

    /// The request for the flag on `target` (an item's identifier or a collection's folder),
    /// as last read.
    func flagRequest(for target: String) -> FlagRequestView? {
        flagRequests.first { view in
            view.flagged.path == target || view.flagged.id == target
        }
    }
}

/// What the librarian is doing about a flag, and what the owner can do: its question to
/// answer, its fixes to accept, why it can't help, and handing it over or looking again.
struct FlagRequestDetail: View {
    let window: ArchiveWindowModel
    let view: FlagRequestView

    @State private var answer = ""
    @State private var working = false
    @State private var problem: String?
    @State private var reporting: ReportingGap?

    private var target: String { view.flagged.id ?? view.flagged.path }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(view.stateText.capitalizedFirst, systemImage: view.state.symbol)
                .font(.callout.bold())
            if !view.said.isEmpty, view.asking == nil {
                Text(view.said).textSelection(.enabled)
            }
            ForEach(Array(view.exchanges.enumerated()), id: \.offset) { _, exchange in
                if let given = exchange.answer {
                    Text("It asked: \(exchange.question)\nYou said: \(given)")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            if let asking = view.asking {
                Text(asking).bold().textSelection(.enabled)
                HStack {
                    TextField("Your answer", text: $answer, axis: .vertical)
                        .lineLimit(1...4)
                        .onSubmit {
                            run {
                                try await $0.answerFlag(
                                    target, answer: answer, memoryGB: LibrarianModel.thisMac)
                            }
                        }
                    Button("Answer") {
                        run {
                            try await $0.answerFlag(
                                target, answer: answer, memoryGB: LibrarianModel.thisMac)
                        }
                    }
                    .disabled(answer.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            ForEach(Array(view.fixes.enumerated()), id: \.offset) { _, fix in
                fixRow(fix)
            }
            if let why = view.cantHelp {
                Text(why).textSelection(.enabled)
                if let missing = view.missing {
                    Text("Carrel would need: \(missing).").font(.caption)
                        .foregroundStyle(.secondary)
                }
                if let gap = view.gap {
                    Button("Report to Development…") { Task { await report(gap) } }
                        .help(
                            "Draft an issue about what Carrel can’t do; nothing is sent until you submit it"
                        )
                }
            }
            HStack {
                Button(view.state == .waiting ? "Ask the Librarian Now" : "Look Again") {
                    run {
                        view.state == .waiting
                            ? try await $0.handFlagToLibrarian(
                                target, memoryGB: LibrarianModel.thisMac)
                            : try await $0.lookAtFlagAgain(target, memoryGB: LibrarianModel.thisMac)
                    }
                }
                .disabled(window.isReadOnly)
                if working { ProgressView().controlSize(.small) }
            }
            if let problem {
                Label(problem, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
        }
        .disabled(working)
        .sheet(item: $reporting) { GapReportSheet(window: window, gap: $0.gap) }
    }

    @ViewBuilder
    private func fixRow(_ fix: FlagFix) -> some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 1) {
                Text(fix.change.capitalizedFirst)
                Text("\(fix.why) (\(Int(fix.confidence * 100))% sure)")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if let number = fix.number {
                Button("Accept") { Task { await accept(number) } }
                    .disabled(window.isReadOnly)
                    .help("Make this change; the flag is cleared once no other fix waits")
            } else if fix.made {
                Label("Made", systemImage: "checkmark").foregroundStyle(.secondary)
            }
        }
    }

    /// Runs the librarian with `work`, then reads the flags again.
    private func run(_ work: @escaping (ArchiveModel) async throws -> FlagRequestView) {
        guard let archive = window.archive else { return }
        working = true
        problem = nil
        Task {
            do {
                _ = try await work(archive)
                answer = ""
            } catch {
                archive.lastError = nil
                problem = (error as? CarrelError)?.message ?? error.localizedDescription
            }
            await window.refreshFlags()
            working = false
        }
    }

    private func accept(_ number: UInt32) async {
        guard let archive = window.archive else { return }
        working = true
        defer { working = false }
        do {
            try await archive.acceptFlagFix(target, number: number)
            await window.refreshFlags()
            await window.browse?.didChange()
        } catch {
            archive.lastError = nil
            problem = (error as? CarrelError)?.message ?? error.localizedDescription
        }
    }

    private func report(_ number: UInt32) async {
        let gaps = (try? await window.archive?.gaps()) ?? []
        if let gap = gaps.first(where: { $0.number == number }) {
            reporting = ReportingGap(gap: gap)
        } else {
            problem = "Gap \(number) isn’t in this Mac’s list any more."
        }
    }
}

/// The librarian's work on a flagged item, under its card in the inspector.
struct ItemFlagSection: View {
    let item: ItemSummary
    let window: ArchiveWindowModel

    var body: some View {
        if let flag = item.flag {
            Section("Needs Work") {
                Label(flag.described, systemImage: "flag.fill").foregroundStyle(.orange)
                    .textSelection(.enabled)
                if let view = window.flagRequest(for: item.id) {
                    FlagRequestDetail(window: window, view: view)
                }
            }
            .task(id: flag.described) { await window.refreshFlags() }
        }
    }
}

extension String {
    /// With its first letter a capital, as a label starts.
    var capitalizedFirst: String {
        prefix(1).uppercased() + dropFirst()
    }
}
