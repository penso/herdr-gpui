// Seals a notification to the public key in $1 (base64url SEC1) the way
// the daemon does (apns/seal.rs) and prints an APNs payload for
// `simctl push`. Usage: swift seal-push.swift KEY > payload.json
import CryptoKit
import Foundation

func b64(_ data: Data) -> String {
    data.base64EncodedString().replacingOccurrences(of: "+", with: "-")
        .replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
}

func decode(_ text: String) -> Data? {
    var base = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
    base += String(repeating: "=", count: (4 - base.count % 4) % 4)
    return Data(base64Encoded: base)
}

let device = try P256.KeyAgreement.PublicKey(x963Representation: decode(CommandLine.arguments[1])!)
let ephemeral = P256.KeyAgreement.PrivateKey()
let info = Data("herdr-remote-notify-v1".utf8)
let key = try ephemeral.sharedSecretFromKeyAgreement(with: device).hkdfDerivedSymmetricKey(
    using: SHA256.self, salt: ephemeral.publicKey.x963Representation + device.x963Representation,
    sharedInfo: info, outputByteCount: 32)
let plaintext = #"{"title":"herdr-gpui · Fix mise discovery","body":"Run git push origin fix/mise?"}"#
let box = try AES.GCM.seal(Data(plaintext.utf8), using: key, authenticating: info)
let sealed = ephemeral.publicKey.x963Representation + Data(box.nonce) + box.ciphertext + box.tag
let payload: [String: Any] = [
    "aps": ["alert": ["title": "Herdr", "body": "New activity"], "mutable-content": 1],
    "sealed": b64(sealed),
]
print(String(decoding: try JSONSerialization.data(withJSONObject: payload), as: UTF8.self))
