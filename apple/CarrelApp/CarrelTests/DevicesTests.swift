// Devices on Status: this device's capability profile written and every device's read, with
// the cards waiting for a stronger device, against a copy of the sample archive.

import CarrelKit
import XCTest

@testable import Carrel

@MainActor
final class DevicesTests: XCTestCase {
    private var sample: SampleCopy!

    override func setUp() async throws {
        sample = try SampleCopy()
    }

    override func tearDown() async throws {
        sample.remove()
    }

    func testOpeningWritesThisDevicesProfileAndListsTheRechecksWaiting() async throws {
        let devices = DevicesModel(archive: sample.archive)
        let facts = devices.facts
        XCTAssertEqual(facts.kind, "mac")
        XCTAssertNotNil(facts.memoryGb)

        await devices.open(writingProfile: true)
        XCTAssertEqual(devices.profiles.count, 2)
        let mine = try XCTUnwrap(devices.profiles.first { $0.thisDevice })
        XCTAssertEqual(mine.kind, "Mac")
        XCTAssertNotNil(mine.benchmarkMbPerSecond)
        // The sample's two cards drafted below the preferred level wait for a stronger device,
        // and this one has no model to do better with.
        XCTAssertEqual(devices.rechecks?.waiting, 2)
        XCTAssertEqual(devices.rechecks?.here, 0)
    }

    func testOpenedWithoutWritingItOnlyReads() async throws {
        let devices = DevicesModel(archive: sample.archive)
        await devices.open(writingProfile: false)
        XCTAssertEqual(devices.profiles.count, 1)
        XCTAssertEqual(devices.rechecks?.waiting, 0)
    }
}
