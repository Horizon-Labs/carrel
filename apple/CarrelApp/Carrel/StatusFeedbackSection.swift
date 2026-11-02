// Status's Feedback (#487): what the owner has sent from any screen on any device, as
// `carrel feedback list` shows it, how many aren't sent yet, the Markdown bundle grouped by
// screen to share, and for each an issue draft that opens in the browser to submit there.
// Carrel posts nothing itself.

import CarrelKit
import SwiftUI

struct StatusFeedbackSection: View {
    let window: ArchiveWindowModel

    @State private var entries: [FeedbackEntry] = []
    @State private var bundle = ""
    @State private var problem: String?

    private var unsent: Int { entries.filter { $0.sent == nil }.count }

    var body: some View {
        Section {
            if entries.isEmpty {
                Text("Nothing sent yet. Send Feedback… is in Help, the ••• menu, or a shake away.")
                    .foregroundStyle(.secondary)
            }
            ForEach(entries.reversed(), id: \.name) { entry in
                LabeledContent {
                    HStack {
                        Button("Issue Draft") { Task { await openDraft(entry) } }
                        Button(entry.sent == nil ? "Mark Sent" : "Unmark") {
                            Task { await mark(entry) }
                        }
                    }
                    .linkButtonStyle()
                } label: {
                    Text(entry.words).lineLimit(2)
                    Text(Self.detail(of: entry))
                }
            }
            if let problem {
                Label(problem, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
        } header: {
            HStack {
                Text(unsent == 0 ? "Feedback" : "Feedback · \(unsent) not sent")
                Spacer()
                if !entries.isEmpty {
                    ShareLink("Share as Markdown", item: bundle)
                        .linkButtonStyle()
                }
                Button("Send Feedback…") { window.startFeedback() }
                    .linkButtonStyle()
            }
        }
        .task(id: "\(window.handle != nil)-\(window.feedback == nil)") { await load() }
    }

    private func load() async {
        do {
            entries = try await window.archive?.feedbackEntries() ?? []
            bundle = try await window.archive?.feedbackExport(unsentOnly: false) ?? ""
            problem = nil
        } catch {
            problem = error.localizedDescription
        }
    }

    private func openDraft(_ entry: FeedbackEntry) async {
        do {
            guard let draft = try await window.archive?.feedbackIssueDraft(entry.name),
                let url = URL(string: draft.url)
            else { return }
            openExternally(url)
        } catch {
            problem = error.localizedDescription
        }
    }

    private func mark(_ entry: FeedbackEntry) async {
        do {
            try await window.archive?.markFeedbackSent(
                entry.name, at: entry.sent == nil ? "" : "-")
            await load()
        } catch {
            problem = error.localizedDescription
        }
    }

    /// An entry's kind, screen, interface and day, in a line.
    static func detail(of entry: FeedbackEntry) -> String {
        var parts = [entry.kind.capitalized, entry.screenPath, entry.interface]
        parts.append(String(entry.at.prefix(10)))
        if entry.screenshotPath != nil { parts.append("screenshot") }
        if let sent = entry.sent { parts.append(sent.isEmpty ? "sent" : "sent: \(sent)") }
        return parts.joined(separator: " · ")
    }
}
