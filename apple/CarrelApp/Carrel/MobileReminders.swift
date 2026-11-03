// Reminders as notifications on iPad and iPhone (docs/design/reminders.md): the reminders due
// (entries waiting, checkouts due back, the week's sum) are posted as local notifications by the
// app and its background tasks, by the rules the Mac agent follows: each once per device, as the
// core records, with permission asked the first time one is due rather than at launch. Choosing
// one opens its item through its `carrel://` link.

#if os(iOS)
import CarrelKit
import Foundation
import Observation
import UserNotifications

/// Posts the open archive's reminders, and holds the link a chosen one opens.
@Observable
final class MobileReminders: NSObject, UNUserNotificationCenterDelegate {
    static let shared = MobileReminders()

    nonisolated private static let linkKey = "link"

    /// The link a chosen notification opens, until the window takes it.
    var chosen: URL?

    /// Becomes the notification center's delegate; called once, as the app launches, so a
    /// notification that launched the app reaches it.
    func start() {
        UNUserNotificationCenter.current().delegate = self
    }

    /// Posts the reminders due in `handle` that this device hasn't shown yet, and marks those
    /// posted shown.
    static func post(_ handle: ArchiveHandle) async {
        guard !CarrelMain.isHostingTests else { return }
        let due = await Task.detached { (try? handle.remindersToShow()) ?? [] }.value
        guard !due.isEmpty else { return }
        let center = UNUserNotificationCenter.current()
        // Asked once; afterwards the owner's answer stands.
        guard (try? await center.requestAuthorization(options: [.alert, .sound])) == true else {
            return
        }
        let root = URL(filePath: handle.root(), directoryHint: .isDirectory)
        let name = handle.name()
        var shown: [String] = []
        for reminder in due {
            let content = UNMutableNotificationContent()
            content.title = reminder.title
            content.subtitle = name
            content.body = reminder.text
            content.userInfo = [
                linkKey: AgentLink(archive: root, item: reminder.item).url.absoluteString
            ]
            let request = UNNotificationRequest(
                identifier: reminder.key, content: content, trigger: nil)
            if (try? await center.add(request)) != nil { shown.append(reminder.key) }
        }
        if !shown.isEmpty {
            let keys = shown
            await Task.detached { try? handle.markRemindersShown(keys: keys) }.value
        }
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
    ) async {
        let info = response.notification.request.content.userInfo
        guard let text = info[Self.linkKey] as? String, let url = URL(string: text),
            var link = AgentLink(url)
        else { return }
        link.archive = MobileLibrary.current(link.archive)
        let chosen = link.url
        await MainActor.run { self.chosen = chosen }
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .list]
    }
}

/// What the app does for Spotlight and reminders whenever the archive may have changed: in the
/// window, and after each background task.
enum MobileArchiveNotices {
    static func refresh(_ handle: ArchiveHandle) async {
        await MobileSpotlight.refresh(handle)
        await MobileReminders.post(handle)
    }
}
#endif
