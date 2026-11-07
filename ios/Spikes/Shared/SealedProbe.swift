import ActivityKit
import CryptoKit
import Foundation
import SheprdRemoteCore
import os

/// A Live Activity whose content only the widget extension can read, the
/// way relay delivery would need (`docs/apns-relay-plan.md`, spike 2). The
/// state holds ciphertext; drawing it opens it with the Secure Enclave key
/// `NotifyKey` keeps in the App Group's keychain group.
struct SealedProbeAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        var sealed: String
    }

    var label: String
}

enum SealedProbe {
    /// How many updates follow the start, and how far apart: they must fit
    /// in the background time iOS gives the app after it locks.
    static let updates = 6
    static let interval = Duration.seconds(4)
    /// A name of its own, as the daemon gives each kind of sealed text.
    static let info = Data("herdr-remote-activity-spike-v1".utf8)
    private static let logName = "sealed-probe.log"

    /// What a real activity's state would hold, at the daemon's longest.
    private struct Plain: Codable {
        var sequence: Int
        var step: String
        var question: String
    }

    /// The sealed state for update `sequence`, to the key in `device`
    /// (X9.63 bytes), as `sheprd-apns/src/seal.rs` seals.
    static func state(sequence: Int, device: Data) throws -> SealedProbeAttributes.ContentState {
        let plain = Plain(
            sequence: sequence,
            step: String(repeating: "s", count: 120),
            question: String(repeating: "q", count: 120)
        )
        let recipient = try P256.KeyAgreement.PublicKey(x963Representation: device)
        let ephemeral = P256.KeyAgreement.PrivateKey()
        let point = ephemeral.publicKey.x963Representation
        let key = try ephemeral.sharedSecretFromKeyAgreement(with: recipient).hkdfDerivedSymmetricKey(
            using: SHA256.self,
            salt: point + device,
            sharedInfo: info,
            outputByteCount: 32
        )
        let box = try AES.GCM.seal(JSONEncoder().encode(plain), using: key, authenticating: info)
        return .init(sealed: Base64URL.encode(point + Data(box.nonce) + box.ciphertext + box.tag))
    }

    /// What drawing one state found, for the lock screen and the log.
    struct Report {
        var headline: String
        var detail: String
    }

    /// Opens `state`, timing it: the check the spike exists for.
    static func open(_ state: SealedProbeAttributes.ContentState) -> Report {
        let started = ContinuousClock.now
        do {
            guard let key = try NotifyKey.load() else { return failed("no key") }
            guard let sealed = Base64URL.decode(state.sealed) else { return failed("bad base64") }
            let bytes = try NotificationSeal.open(
                sealed,
                device: key.publicKey,
                info: info,
                agree: key.agree
            )
            let plain = try JSONDecoder().decode(Plain.self, from: bytes)
            let time = started.duration(to: .now).components
            let ms = time.seconds * 1000 + time.attoseconds / 1_000_000_000_000_000
            let kind = if case .secureEnclave = key { "Secure Enclave" } else { "software key" }
            let free = os_proc_available_memory() / 1_000_000
            let report = Report(
                headline: "Opened #\(plain.sequence) in \(ms) ms",
                detail: "\(kind), \(free) MB free"
            )
            log("widget \(report.headline), \(report.detail)")
            return report
        } catch {
            return failed("\(error)")
        }
    }

    private static func failed(_ why: String) -> Report {
        log("widget FAILED: \(why)")
        return Report(headline: "FAILED", detail: why)
    }

    private static var logFile: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: SpikeIDs.appGroup)?
            .appending(path: logName)
    }

    /// Adds a line both the app and the extension can read later. Best
    /// effort: the lock screen shows the result whether or not this works.
    static func log(_ line: String) {
        guard let file = logFile else { return }
        let stamped = Data("\(Date().formatted(.iso8601)) \(line)\n".utf8)
        guard let handle = try? FileHandle(forWritingTo: file) else {
            try? stamped.write(to: file)
            return
        }
        defer { try? handle.close() }
        _ = try? handle.seekToEnd()
        try? handle.write(contentsOf: stamped)
    }

    static func readLog() -> String {
        logFile.flatMap { try? String(contentsOf: $0, encoding: .utf8) } ?? "no log yet"
    }

    static func clearLog() {
        logFile.map { try? FileManager.default.removeItem(at: $0) }
    }
}
