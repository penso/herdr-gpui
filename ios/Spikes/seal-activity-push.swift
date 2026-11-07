// How much of an APNs Live Activity payload sealing uses: seals the largest
// content the daemon writes (`apns/decide.rs`, `apns/payload.rs`) the way
// `sheprd-apns/src/seal.rs` does, with a throwaway key, and prints each push's size
// against APNs's 4,096 bytes. Usage: swift seal-activity-push.swift
import CryptoKit
import Foundation

let info = Data("herdr-remote-activity-spike-v1".utf8)

func b64(_ data: Data) -> String {
    data.base64EncodedString().replacingOccurrences(of: "+", with: "-")
        .replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
}

func seal(_ plaintext: Data, to device: P256.KeyAgreement.PublicKey) throws -> String {
    let ephemeral = P256.KeyAgreement.PrivateKey()
    let point = ephemeral.publicKey.x963Representation
    let key = try ephemeral.sharedSecretFromKeyAgreement(with: device).hkdfDerivedSymmetricKey(
        using: SHA256.self, salt: point + device.x963Representation,
        sharedInfo: info, outputByteCount: 32)
    let box = try AES.GCM.seal(plaintext, using: key, authenticating: info)
    return b64(point + Data(box.nonce) + box.ciphertext + box.tag)
}

func json(_ value: Any) throws -> Data {
    try JSONSerialization.data(withJSONObject: value)
}

/// `EXCERPT_CHARS` characters of `letter`.
func text(_ letter: String) -> String { String(repeating: letter, count: 120) }

func contentState(_ letter: String) -> [String: Any] {
    [
        "phase": "question", "step": text(letter), "startedAt": 1_800_000_000,
        "question": text(letter), "summary": text(letter), "planDone": 99, "planTotal": 99,
        "working": 99, "waiting": 99, "permission": 1_000_000,
        "previous": [text(letter), text(letter)],
        "options": (0..<3).map { ["id": "option-\($0)", "kind": "allow_once", "name": String(text(letter).prefix(40))] },
    ]
}

func attributes(_ letter: String) -> [String: Any] {
    [
        "agent": String(text(letter).prefix(40)), "chatTitle": text(letter), "computer": text(letter),
        "computerId": String(repeating: "A", count: 64), "pane": String(repeating: "p", count: 128),
        "workspace": text(letter),
    ]
}

let device = P256.KeyAgreement.PrivateKey().publicKey
let cover: [String: Any] = ["title": "Sheprd", "body": "New activity"]

for (name, letter) in [("ASCII", "a"), ("3-byte text (CJK)", "\u{6f22}")] {
    let state = ["sealed": try seal(json(contentState(letter)), to: device)]
    let attrs = ["sealed": try seal(json(attributes(letter)), to: device)]
    let update: [String: Any] = ["aps": ["timestamp": 1_800_000_000, "event": "update", "content-state": state, "alert": cover]]
    // A start has no step or question yet (`decide`, `Change::Turn`).
    let first: [String: Any] = ["phase": "working", "step": "Working", "startedAt": 1_800_000_000, "working": 99, "waiting": 99]
    let start: [String: Any] = ["aps": [
        "timestamp": 1_800_000_000, "event": "start", "attributes-type": "ChatActivityAttributes",
        "attributes": attrs, "content-state": ["sealed": try seal(json(first), to: device)], "alert": cover,
    ]]
    print("\(name): plain state \(try json(contentState(letter)).count) B, attributes \(try json(attributes(letter)).count) B")
    print("  sealed update \(try json(update).count) of 4096 B, sealed start \(try json(start).count) of 4096 B")
}
