import Foundation
import SheprdRemoteCore
import UserNotifications

/// Opens a sealed alert (`SealedAlert`) with the key the app registered,
/// so APNs only ever carried "Sheprd" and ciphertext, then shows the chat's
/// title and text, groups it with the chat's other notifications, and
/// gives it the actions its question or finished turn offers.
final class NotificationService: UNNotificationServiceExtension {
    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        let content = (request.content.mutableCopy() as? UNMutableNotificationContent) ?? UNMutableNotificationContent()
        guard let sealed = request.content.userInfo["sealed"] as? String,
              let opened = try? NotifyKey.open(sealed)
        else {
            // Not from a paired computer, or one this phone forgot. Only
            // the cover is shown, whatever the push said, with none of its
            // actions: opening the app shows what is really waiting.
            contentHandler(Self.cover())
            return
        }
        let alert = opened.alert
        content.title = alert.title
        content.body = alert.body
        content.threadIdentifier = AlertKeys.thread(computer: opened.computer.hex, pane: alert.pane)
        // The count is this computer's; the badge is every computer's.
        if let badge = alert.badge {
            let total = BadgeCounts.update(badge, for: alert.computer, in: UserDefaults(suiteName: "group.so.pen.herdr.remote"))
            content.badge = NSNumber(value: total)
        }
        var info = content.userInfo
        info["sealed"] = nil
        // The computer whose key opened it, which actions then reach.
        info[AlertKeys.computer] = opened.computer.hex
        info[AlertKeys.pane] = alert.pane
        if let permission = alert.permission { info[AlertKeys.permission] = NSNumber(value: permission.id.value) }
        content.userInfo = info
        let presentation = alert.presentation
        guard let category = presentation.category else {
            contentHandler(content)
            return
        }
        content.categoryIdentifier = category
        let deliver = UncheckedSendable(value: contentHandler)
        let finished = UncheckedSendable(value: content)
        Task {
            await NotificationCategories.register(presentation)
            deliver.value(finished.value)
        }
    }

    /// What every sealed alert says on the way here, built fresh so nothing
    /// of the push survives: text, actions, badge, or data an action reads.
    private static func cover() -> UNNotificationContent {
        let content = UNMutableNotificationContent()
        content.title = "Sheprd"
        content.body = "New activity"
        return content
    }
}

/// Lets a value iOS hands the extension cross into the task that finishes
/// it; the handler is called once, after the category is registered.
private struct UncheckedSendable<Value>: @unchecked Sendable {
    let value: Value
}
