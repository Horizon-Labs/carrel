// Devices on Status (docs/design/sync-and-mobile.md, "Capability profiles and the quality
// ladder"): this device's capability profile is written when the window opens an archive;
// Status lists every device's, and the cards waiting for a stronger one, which this device
// re-checks now when it can do better, as `carrel devices recheck` does.

import CarrelKit
import Foundation
import Observation
import SwiftUI

#if os(macOS)
import IOKit.ps
#else
import UIKit
#endif

@MainActor
@Observable
final class DevicesModel {
    let archive: ArchiveModel

    /// Every device's profile, newest first, once read.
    private(set) var profiles: [DeviceProfileRecord] = []
    /// The cards waiting for a stronger device, once read.
    private(set) var rechecks: RechecksRecord?
    /// What the last re-check did.
    private(set) var report: String?

    init(archive: ArchiveModel) {
        self.archive = archive
    }

    /// What the platform says about this device now.
    var facts: DeviceFactsRecord {
        #if os(macOS)
        let kind = "mac"
        let name = Host.current().localizedName
        #else
        let kind = PowerPolicy.isPhone ? "iphone" : "ipad"
        let name: String? = UIDevice.current.name
        #endif
        return .measured(
            root: URL(fileURLWithPath: archive.handle.root()), name: name, kind: kind,
            onPower: Self.onPower)
    }

    /// Whether this device is on mains power, when it can tell.
    private static var onPower: Bool? {
        #if os(macOS)
        guard let info = IOPSCopyPowerSourcesInfo()?.takeRetainedValue(),
            let type = IOPSGetProvidingPowerSourceType(info)?.takeUnretainedValue()
        else { return nil }
        return (type as String) == kIOPMACPowerKey
        #else
        let device = UIDevice.current
        device.isBatteryMonitoringEnabled = true
        switch device.batteryState {
        case .charging, .full: return true
        case .unplugged: return false
        default: return nil
        }
        #endif
    }

    /// Writes this device's profile when `writingProfile`, then reads every device's.
    func open(writingProfile: Bool) async {
        if writingProfile { try? await archive.updateDeviceProfile(facts) }
        await refresh()
    }

    /// Reads every device's profile and the re-check queue again.
    func refresh() async {
        profiles = (try? await archive.deviceProfiles()) ?? []
        rechecks = try? await archive.rechecks()
    }

    /// Re-checks a few of the cards waiting that this device can do better.
    func recheckNow() async {
        report = nil
        do {
            let done = try await archive.recheckNow(
                facts: facts, memoryGB: LibrarianModel.thisMac)
            report =
                done.summary.map {
                    "\(thisDevice.prefix(1).uppercased() + thisDevice.dropFirst()) \($0)."
                }
                ?? "Nothing was waiting that \(thisDevice) can do better."
            if !done.failed.isEmpty { report? += " " + done.failed.joined(separator: "; ") }
        } catch {
            archive.lastError = nil
            report = message(for: error)
        }
        await refresh()
    }
}

struct StatusDevicesSection: View {
    let window: ArchiveWindowModel

    var body: some View {
        if let devices = window.devices, !devices.profiles.isEmpty {
            Section("Devices") {
                ForEach(devices.profiles, id: \.id) { profile in
                    DisclosureGroup {
                        DeviceProfileDetails(profile: profile)
                    } label: {
                        LabeledContent(
                            profile.thisDevice ? "\(profile.name) (\(thisDevice))" : profile.name,
                            value: profile.kind)
                    }
                }
                if let rechecks = devices.rechecks, rechecks.waiting > 0 {
                    DisclosureGroup(
                        rechecks.waiting == 1
                            ? "1 card waiting for a stronger device"
                            : "\(rechecks.waiting) cards waiting for a stronger device"
                    ) {
                        ForEach(rechecks.cards.prefix(20), id: \.self) { card in
                            Text(card).font(.callout).textSelection(.enabled)
                        }
                    }
                    .help(
                        "Drafted on a weaker device or with a weaker model; a stronger device re-checks them and proposes better drafts in Review"
                    )
                    if rechecks.claimed > 0 {
                        Text("\(rechecks.claimed) being re-checked by another device")
                            .foregroundStyle(.secondary)
                    }
                    if rechecks.here > 0 {
                        Button("Re-check Some Now") { Task { await devices.recheckNow() } }
                            .help(
                                "\(thisDevice.prefix(1).uppercased() + thisDevice.dropFirst()) can do better on \(rechecks.here) of them"
                            )
                            .disabled(window.isReadOnly || devices.archive.isWorking)
                    }
                }
                if let report = devices.report {
                    Text(report).font(.callout).foregroundStyle(.secondary)
                }
            }
        }
    }
}

private struct DeviceProfileDetails: View {
    let profile: DeviceProfileRecord

    var body: some View {
        if let chip = profile.chip { LabeledContent("Chip", value: chip) }
        if let memory = profile.memoryGb { LabeledContent("Memory", value: "\(memory) GB") }
        if let free = profile.freeGb { LabeledContent("Free storage", value: "\(free) GB") }
        LabeledContent(
            "Models",
            value: profile.models.isEmpty ? "None" : profile.models.joined(separator: ", "))
        LabeledContent(
            "Tools", value: profile.tools.isEmpty ? "None" : profile.tools.joined(separator: ", "))
        if let power = powerLine { LabeledContent("Power", value: power) }
        if let background = profile.background {
            LabeledContent("Background time", value: background.capitalized)
        }
        if let speed = profile.benchmarkMbPerSecond {
            LabeledContent("Self-benchmark", value: "\(speed) MB/s")
        }
        if let updated = profile.updated {
            LabeledContent("Updated", value: String(updated.prefix(10)))
        }
    }

    private var powerLine: String? {
        var parts: [String] = []
        if let onPower = profile.onPower { parts.append(onPower ? "On power" : "On battery") }
        if profile.lowPower == true { parts.append("Low Power Mode") }
        if let thermal = profile.thermal { parts.append("\(thermal.capitalized) heat") }
        return parts.isEmpty ? nil : parts.joined(separator: ", ")
    }
}
