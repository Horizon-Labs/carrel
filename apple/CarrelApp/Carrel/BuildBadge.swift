// The flag a test build shows below the sidebar, such as "v1.0 Beta (5a81b54c1d)": its version
// and the git commit it was built from, so a report names the exact build. The build sets the
// commit (CARREL_BUILD, which scripts/testflight.sh passes) and the channel (CARREL_CHANNEL); a
// release has no channel and shows no flag.

import SwiftUI

enum BuildInfo {
    /// The test build's flag, or nil for a release.
    static let label: String? = {
        let info = Bundle.main.infoDictionary ?? [:]
        guard let channel = info["CarrelChannel"] as? String, !channel.isEmpty else { return nil }
        let version = (info["CFBundleShortVersionString"] as? String ?? "")
            .split(separator: ".").prefix(2).joined(separator: ".")
        let commit = (info["CarrelBuild"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        return "v\(version) \(channel) (\(commit ?? "local build"))"
    }()
}

/// The test build's flag, when it is one.
struct BuildBadge: View {
    var body: some View {
        if let label = BuildInfo.label {
            Text(label)
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(.orange.opacity(0.15), in: Capsule())
                .textSelection(.enabled)
                .help("A test build. Name it when you report a problem.")
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
        }
    }
}
