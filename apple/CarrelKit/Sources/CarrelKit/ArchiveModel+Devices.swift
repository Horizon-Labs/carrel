// Capability profiles and the cross-device re-check queue (docs/design/sync-and-mobile.md,
// "Capability profiles and the quality ladder"): this device's profile, measured with what only
// the platform can tell and written when an archive opens; every device's, and the cards
// waiting for a stronger one, for Status; and re-checking them now.

import Foundation

extension DeviceFactsRecord {
    /// What this device's platform says about it, for the archive at `root`: its chip, memory,
    /// free storage, low-power mode, heat and background time. `name`, `kind` (`mac`, `ipad`
    /// or `iphone`) and `onPower` come from the app, which can ask the platform's UI framework.
    public static func measured(
        root: URL, name: String? = nil, kind: String? = nil, onPower: Bool? = nil
    ) -> DeviceFactsRecord {
        let info = ProcessInfo.processInfo
        let bytes = info.physicalMemory
        let free = try? root.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
            .volumeAvailableCapacityForImportantUsage
        #if os(macOS)
            let background = "unlimited"
        #else
            let background = "limited"
        #endif
        return DeviceFactsRecord(
            name: name,
            kind: kind,
            chip: chip(),
            memoryGb: bytes > 0 ? (bytes + (1 << 29)) >> 30 : nil,
            freeGb: free.flatMap { $0 }.map { UInt64(max($0, 0)) >> 30 },
            onPower: onPower,
            lowPower: info.isLowPowerModeEnabled,
            thermal: thermal(info.thermalState),
            background: background
        )
    }

    /// The chip's name on a Mac, or the model's identifier on an iPad or iPhone.
    private static func chip() -> String? {
        #if os(macOS)
            let key = "machdep.cpu.brand_string"
        #else
            let key = "hw.machine"
        #endif
        var size = 0
        guard sysctlbyname(key, nil, &size, nil, 0) == 0, size > 0 else { return nil }
        var bytes = [CChar](repeating: 0, count: size)
        guard sysctlbyname(key, &bytes, &size, nil, 0) == 0 else { return nil }
        let name = String(decoding: bytes.prefix { $0 != 0 }.map(UInt8.init), as: UTF8.self)
        return name.isEmpty ? nil : name
    }

    private static func thermal(_ state: ProcessInfo.ThermalState) -> String? {
        switch state {
        case .nominal: "nominal"
        case .fair: "fair"
        case .serious: "serious"
        case .critical: "critical"
        @unknown default: nil
        }
    }
}

extension ArchiveModel {
    /// Writes this device's profile, measured with `facts`, when it changed or is a day old,
    /// and brings the re-check queue up to date unless another device is writing.
    public func updateDeviceProfile(_ facts: DeviceFactsRecord) async throws {
        try await call { try $0.updateDeviceProfile(facts: facts) }
    }

    /// Every device's profile, newest first.
    public func deviceProfiles() async throws -> [DeviceProfileRecord] {
        try await call { try $0.deviceProfiles() }
    }

    /// The cards waiting for a stronger device.
    public func rechecks() async throws -> RechecksRecord {
        try await call { try $0.rechecks() }
    }

    /// Re-checks up to `cards` cards weaker devices drafted, as this device measured with
    /// `facts`, with `model` or its strongest that fits in `memoryGB`. Better drafts wait in
    /// Review.
    @discardableResult
    public func recheckNow(
        cards: UInt32 = 4, facts: DeviceFactsRecord, model: ModelProvider? = nil,
        memoryGB: UInt64?
    ) async throws -> RecheckedRecord {
        let done = try await call(reporting: "Re-checking cards weaker devices drafted") {
            handle, _ in
            try handle.recheckNow(
                cards: cards, facts: facts, model: model, memoryGb: memoryGB, stop: nil)
        }
        try await refreshCrawler()
        return done
    }
}
