import HerdrRemoteCore
import os
import UIKit
import UserNotifications

private let log = Logger(subsystem: "so.pen.herdr.remote", category: "alerts")

/// Owns the app model, so notification actions and pushes that launch the
/// app in the background reach the same state the screens use, and
/// handles the app's APNs token, sealed alerts' actions, and clears.
@MainActor
final class AppDelegate: NSObject, UIApplicationDelegate, @preconcurrency UNUserNotificationCenterDelegate {
    let model = AppModel()

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        AppLock.migrate()
        AppModel.current = model
        UNUserNotificationCenter.current().delegate = self
        Task { await NotificationCategories.register(.finished) }
        #if DEBUG
        if followDebugArguments() { return true }
        #endif
        if !model.computers.isEmpty { Task { await Self.enableAlerts() } }
        return true
    }

    /// Asks once to show alerts, then registers for the APNs token the
    /// paired computers seal alerts to.
    static func enableAlerts() async {
        let center = UNUserNotificationCenter.current()
        var options: UNAuthorizationOptions = [.alert, .badge, .sound]
        #if DEBUG
        // The simulator loop cannot tap the prompt (`-provisionalAlerts`).
        if UserDefaults.standard.bool(forKey: "provisionalAlerts") { options.insert(.provisional) }
        #endif
        let granted = (try? await center.requestAuthorization(options: options)) ?? false
        if granted { UIApplication.shared.registerForRemoteNotifications() }
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        model.tokens.setAlert(deviceToken.hexString)
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        log.error("No APNs token: \(error.localizedDescription, privacy: .public)")
    }

    /// A background push: a question was answered elsewhere, so the chat's
    /// notifications go and the badge follows.
    func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable: Any]
    ) async -> UIBackgroundFetchResult {
        guard let sealed = userInfo["sealed"] as? String,
              let opened = try? NotifyKey.open(sealed)
        else { return .noData }
        let alert = opened.alert
        let center = UNUserNotificationCenter.current()
        if alert.clear {
            // Only the computer whose key opened the clear: another may use
            // the same pane id for a chat still waiting.
            let shown = await center.deliveredNotifications()
            let gone = shown.filter { AlertKeys.belongs($0.request.content, computer: opened.computer.hex, pane: alert.pane) }
                .map(\.request.identifier)
            center.removeDeliveredNotifications(withIdentifiers: gone)
        }
        if let badge = alert.badge {
            let total = BadgeCounts.update(badge, for: alert.computer, in: UserDefaults(suiteName: "group.so.pen.herdr.remote"))
            try? await center.setBadgeCount(total)
        }
        return .newData
    }

    // MARK: UNUserNotificationCenterDelegate

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound, .badge]
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let info = response.notification.request.content.userInfo
        guard let computer = info[AlertKeys.computer] as? String, let pane = info[AlertKeys.pane] as? String else { return }
        if response.actionIdentifier == UNNotificationDefaultActionIdentifier {
            let name = model.computers.first { $0.fingerprint == Fingerprint(computer) }?.name ?? ""
            model.open(ChatLink(computer: name, computerId: computer, pane: pane).url)
            return
        }
        let permission = (info[AlertKeys.permission] as? NSNumber).map { PermissionId($0.uint64Value) }
        let text = (response as? UNTextInputNotificationResponse)?.userText
        guard let command = AlertPresentation.command(action: response.actionIdentifier, permission: permission, text: text) else { return }
        do {
            try await BackgroundCommands.send(command, computer: computer, pane: pane)
        } catch {
            log.error("Could not send from a notification: \(error.localizedDescription, privacy: .public)")
            let failed = UNMutableNotificationContent()
            failed.title = "Not sent"
            failed.body = error.localizedDescription
            failed.threadIdentifier = AlertKeys.thread(computer: computer, pane: pane)
            try? await center.add(UNNotificationRequest(identifier: UUID().uuidString, content: failed, trigger: nil))
        }
    }
}

#if DEBUG
extension AppDelegate {
    /// For the simulator loop, which cannot tap the permission prompt:
    /// `-provisionalAlerts YES` asks quietly instead, `-exportNotifyKey FP`
    /// writes the public half of computer FP's alert key to the App Group, and
    /// `-dumpNotifications YES` writes what was delivered there, as the
    /// extension left it. Answers whether it handled alerts itself.
    func followDebugArguments() -> Bool {
        let defaults = UserDefaults.standard
        // `-lockForTest YES|NO` turns the Face ID lock on or off.
        if defaults.object(forKey: "lockForTest") != nil {
            AppLock.store?.set(defaults.bool(forKey: "lockForTest"), forKey: AppLock.setting)
        }
        guard let folder = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: NotifyKey.group) else { return false }
        if let hex = defaults.string(forKey: "exportNotifyKey"), let computer = Fingerprint(hex),
           let key = try? NotifyKey.loadOrCreate(for: computer) {
            try? Data(key.registered.utf8).write(to: folder.appending(path: "debug-notify-key.txt"))
        }
        if defaults.bool(forKey: "dumpNotifications") {
            Task {
                let shown = await UNUserNotificationCenter.current().deliveredNotifications()
                let rows = shown.map { note -> [String: String] in
                    let content = note.request.content
                    return [
                        "title": content.title, "body": content.body,
                        "category": content.categoryIdentifier, "thread": content.threadIdentifier,
                        "pane": content.userInfo[AlertKeys.pane] as? String ?? "",
                        "permission": (content.userInfo[AlertKeys.permission] as? NSNumber)?.stringValue ?? "",
                        "sealed_left": content.userInfo["sealed"] == nil ? "no" : "yes",
                        // The simulator does not run the extension for
                        // `simctl push`; this opens it as the extension would.
                        "opened": (content.userInfo["sealed"] as? String).flatMap { sealed in
                            (try? NotifyKey.open(sealed))?.map { "\($0.computer.hex.prefix(4)): \($0.alert.title) | \($0.alert.body) | \($0.alert.presentation.actions.map(\.title))" }
                        } ?? "",
                    ]
                }
                let categories = await UNUserNotificationCenter.current().notificationCategories()
                    .map { "\($0.identifier): \($0.actions.map(\.title).joined(separator: ", "))" }
                let dump = ["delivered": rows, "categories": categories.map { ["category": $0] }]
                if let data = try? JSONSerialization.data(withJSONObject: dump, options: [.prettyPrinted, .sortedKeys]) {
                    try? data.write(to: folder.appending(path: "debug-delivered.json"))
                }
            }
        }
        guard defaults.bool(forKey: "provisionalAlerts") else { return false }
        Task {
            _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.provisional, .alert, .badge, .sound])
            UIApplication.shared.registerForRemoteNotifications()
        }
        return true
    }
}
#endif
