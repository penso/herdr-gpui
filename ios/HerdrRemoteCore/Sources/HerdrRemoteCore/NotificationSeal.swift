import CryptoKit
import Foundation

/// Opens a notification payload the daemon sealed to this device
/// (`crates/herdr-remote-control/src/apns/seal.rs`), so APNs only relays
/// ciphertext: `ephemeral point (65) || nonce (12) || ciphertext || tag`,
/// AES-256-GCM under `HKDF-SHA256(salt: ephemeral || device, info)` of the
/// ECDH secret, with `info` as associated data.
public enum NotificationSeal {
    public static let info = Data("herdr-remote-notify-v1".utf8)
    private static let pointLength = 65
    private static let nonceLength = 12
    private static let tagLength = 16

    public enum Failure: Error, Equatable {
        /// Too short, or not a P-256 point where the sender's key belongs.
        case format
        /// The key or the bytes do not match: tampered, or sealed to another device.
        case open
    }

    /// The plaintext of `sealed`. `agree` performs ECDH with the device's
    /// key-agreement key, which may live in the Secure Enclave.
    /// `info` names what was sealed: alerts use `info`, a computer's
    /// addresses `Presence.addressesInfo`, so one cannot pass for the other.
    public static func open(
        _ sealed: Data,
        device: P256.KeyAgreement.PublicKey,
        info: Data = NotificationSeal.info,
        agree: (P256.KeyAgreement.PublicKey) throws -> SharedSecret
    ) throws -> Data {
        let bytes = Data(sealed)
        guard bytes.count >= pointLength + nonceLength + tagLength else { throw Failure.format }
        let point = bytes.prefix(pointLength)
        let nonce = bytes.dropFirst(pointLength).prefix(nonceLength)
        let body = bytes.dropFirst(pointLength + nonceLength)
        guard let ephemeral = try? P256.KeyAgreement.PublicKey(x963Representation: point) else {
            throw Failure.format
        }
        let key = try agree(ephemeral).hkdfDerivedSymmetricKey(
            using: SHA256.self,
            salt: point + device.x963Representation,
            sharedInfo: info,
            outputByteCount: 32
        )
        do {
            let box = try AES.GCM.SealedBox(
                nonce: AES.GCM.Nonce(data: nonce),
                ciphertext: body.dropLast(tagLength),
                tag: body.suffix(tagLength)
            )
            return try AES.GCM.open(box, using: key, authenticating: info)
        } catch {
            throw Failure.open
        }
    }
}
