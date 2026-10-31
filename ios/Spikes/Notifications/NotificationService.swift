import CryptoKit
import Foundation
import HerdrRemoteCore
import UserNotifications

/// Opens `sealed` (base64url) with the key the app stored, and shows the
/// plaintext's title and body; APNs only ever carried ciphertext.
final class NotificationService: UNNotificationServiceExtension {
    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        let content = (request.content.mutableCopy() as? UNMutableNotificationContent) ?? UNMutableNotificationContent()
        do {
            guard let text = request.content.userInfo["sealed"] as? String,
                  let sealed = Base64URL.decode(text)
            else { throw NotificationSeal.Failure.format }
            guard let key = try NotifyKey.load() else { throw NotifyKey.Failure.keychain(errSecItemNotFound) }
            let opened = try NotificationSeal.open(sealed, device: key.publicKey) { try key.agree($0) }
            let message = try JSONDecoder().decode(Message.self, from: opened)
            content.title = message.title
            content.body = message.body
        } catch {
            content.body = "Could not open: \(error)"
        }
        contentHandler(content)
    }

    private struct Message: Decodable {
        let title: String
        let body: String
    }
}
