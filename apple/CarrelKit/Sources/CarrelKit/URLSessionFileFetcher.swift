// Downloading a catalog model's files where the core can't start its network helper,
// `carrel-fetch`: on iPad and iPhone (#481). The core says what to fetch, from which hosts, and
// checks each file's size and SHA-256 once it's written; this only moves the bytes, continuing
// a partial file, and refuses any host or redirect the core didn't allow.

import Foundation

/// The core's [`FileFetcher`] through `URLSession`.
public final class URLSessionFileFetcher: FileFetcher, @unchecked Sendable {
    private let session: URLSession

    public init(session: URLSession = .shared) {
        self.session = session
    }

    public func fetch(download: FileDownload, progress: ProgressListener) throws {
        guard let url = URL(string: download.url), url.scheme == "https",
            let host = url.host(), downloadHostAllowed(host: host, hosts: download.hosts)
        else {
            throw CarrelError.Callback(message: "won't download from \(download.url)")
        }
        nonisolated(unsafe) var outcome: Error? = CarrelError.Callback(message: "no answer")
        let done = DispatchSemaphore(value: 0)
        let session = session
        Task.detached(priority: .utility) {
            do {
                try await Self.copy(url, download, progress, session)
                outcome = nil
            } catch {
                outcome = error
            }
            done.signal()
        }
        done.wait()
        if let outcome {
            throw outcome as? CarrelError ?? CarrelError.Callback(message: "\(outcome)")
        }
    }

    /// Writes `url` to the download's file, continuing from the file's end.
    private static func copy(
        _ url: URL, _ download: FileDownload, _ progress: ProgressListener, _ session: URLSession
    ) async throws {
        let file = URL(filePath: download.to)
        let fm = FileManager.default
        var have = (try? fm.attributesOfItem(atPath: download.to)[.size] as? UInt64) ?? 0
        if have >= download.size {
            if have == download.size { return }
            try fm.removeItem(at: file)
            have = 0
        }
        var request = URLRequest(url: url)
        if have > 0 { request.setValue("bytes=\(have)-", forHTTPHeaderField: "Range") }
        let (bytes, response) = try await session.bytes(
            for: request, delegate: RedirectGuard(hosts: download.hosts))
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        switch status {
        case 206: break
        case 200: have = 0
        default: throw CarrelError.Callback(message: "\(url.host() ?? "the server") said \(status)")
        }
        if have == 0 { fm.createFile(atPath: download.to, contents: nil) }
        let handle = try FileHandle(forWritingTo: file)
        defer { try? handle.close() }
        try handle.truncate(atOffset: have)
        try handle.seekToEnd()
        let chunk = 1 << 20
        var buffer = Data(capacity: chunk)
        func flush() throws {
            try handle.write(contentsOf: buffer)
            have += UInt64(buffer.count)
            buffer.removeAll(keepingCapacity: true)
            progress.update(done: have, total: download.size, item: nil)
            if have > download.size {
                throw CarrelError.Callback(message: "the file is larger than the catalog says")
            }
        }
        for try await byte in bytes {
            buffer.append(byte)
            if buffer.count >= chunk { try flush() }
            try Task.checkCancellation()
        }
        try flush()
    }
}

/// Follows a redirect only to a host the core allowed.
private final class RedirectGuard: NSObject, URLSessionTaskDelegate, Sendable {
    let hosts: [String]

    init(hosts: [String]) {
        self.hosts = hosts
    }

    func urlSession(
        _ session: URLSession, task: URLSessionTask,
        willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest
    ) async -> URLRequest? {
        guard let url = request.url, url.scheme == "https", let host = url.host(),
            downloadHostAllowed(host: host, hosts: hosts)
        else { return nil }
        return request
    }
}
