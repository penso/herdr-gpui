import CryptoKit
import Foundation

/// What a device signs to start a session: `herdr-remote-session\0` then the
/// challenge nonce's 32 decoded bytes. The prefix makes the device key's
/// signature over a nonce mean nothing anywhere else.
public enum SessionMessage {
    public static let prefix = Data("herdr-remote-session\u{0}".utf8)

    public enum Failure: Error, Equatable {
        case nonce
    }

    /// The bytes to sign for the base64url `nonce` a challenge carries.
    public static func bytes(nonce: String) throws -> Data {
        guard let decoded = Base64URL.decode(nonce), decoded.count == 32 else {
            throw Failure.nonce
        }
        return prefix + decoded
    }
}

/// The encodings the daemon verifies a device key in.
public enum DeviceKeyEncoding {
    /// The public key as SEC1 uncompressed bytes (`0x04 || X || Y`, 65
    /// bytes), base64url: the `public_key` of a pairing claim.
    public static func publicKey(_ key: P256.Signing.PublicKey) -> String {
        Base64URL.encode(key.x963Representation)
    }

    /// An ECDSA P-256 SHA-256 signature as IEEE P1363 `r || s` (64 bytes),
    /// base64url: the `signature` of a session request.
    public static func signature(_ signature: P256.Signing.ECDSASignature) -> String {
        Base64URL.encode(signature.rawRepresentation)
    }
}
