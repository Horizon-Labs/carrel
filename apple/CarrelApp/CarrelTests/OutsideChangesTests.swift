// An archive window follows changes made outside it: by the command line or the TUI, the
// worker, or another device through iCloud.

import CarrelKit
import XCTest

@testable import Carrel

@MainActor
final class OutsideChangesTests: XCTestCase {
    func testOnlyChangesThatCanShowReloadTheWindow() {
        let root = "/Users/me/Arx"
        for path in [
            "\(root)/Holding/Rulebook.pdf.md", "\(root)/_System/queue/review/01a1.json",
            "\(root)/_System/history.git/refs/heads/main", "\(root)/ARCHIVE.md",
        ] {
            XCTAssertTrue(ArchiveWindowModel.shows(path), path)
        }
        for path in [
            "\(root)/_System/cache.nosync/text/f2ea.txt", "\(root)/_System/index/model/37.shard",
            "\(root)/_System/devices/01a1/device.json",
            "\(root)/_System/history.git/objects/59/89d4", "\(root)/_System/history.git/index",
            "\(root)/Holding/.DS_Store",
        ] {
            XCTAssertFalse(ArchiveWindowModel.shows(path), path)
        }
    }

    #if os(macOS)
    func testTheWindowShowsACollectionMadeOutsideIt() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appending(path: "carrel-outside-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let root = folder.appending(path: "archive")
        let config = folder.appending(path: "config")
        try FileManager.default.createDirectory(at: config, withIntermediateDirectories: true)
        try FileManager.default.copyItem(at: sampleArchive, to: root)
        // Another process's handle, as the command line would have.
        let other = try ArchiveHandle.open(root, options: .isolated(config: config))
        try other.startHistory()
        let model = ArchiveWindowModel(url: root, options: .isolated(config: config))
        await model.load()
        XCTAssertNil(model.problem)
        let following = Task { await model.followOutsideChanges() }
        defer { following.cancel() }
        try await Task.sleep(for: .milliseconds(500))

        _ = try other.createCollection(path: "Made Elsewhere", description: nil, tags: [])

        for _ in 0..<50 where !model.collections.contains(where: { $0.path == "Made Elsewhere" }) {
            try await Task.sleep(for: .milliseconds(200))
        }
        XCTAssertTrue(model.collections.contains { $0.path == "Made Elsewhere" })
    }
    #endif
}
