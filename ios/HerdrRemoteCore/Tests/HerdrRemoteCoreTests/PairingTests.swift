import CryptoKit
import XCTest
@testable import HerdrRemoteCore

final class PairingTests: XCTestCase {
    private let hex = String(repeating: "AB", count: 32)

    func testParsesThePairingQR() throws {
        let link = try PairingLink("https://192.168.1.20:7443/pair#t=abc-DEF_123&fp=\(hex)")
        XCTAssertEqual(link.origin, "https://192.168.1.20:7443")
        XCTAssertEqual(link.token, "abc-DEF_123")
        XCTAssertEqual(link.fingerprint.hex, hex)
    }

    func testAcceptsPastedTextWithWhitespaceAndColonFingerprints() throws {
        let colons = Array(repeating: "ab", count: 32).joined(separator: ":")
        let link = try PairingLink("  https://studio.local:7443/pair#fp=\(colons)&t=tok\n")
        XCTAssertEqual(link.origin, "https://studio.local:7443")
        XCTAssertEqual(link.fingerprint.hex, hex)
    }

    func testKeepsIPv6Brackets() throws {
        let link = try PairingLink("https://[fd7a::3]:7443/pair#t=tok&fp=\(hex)")
        XCTAssertEqual(link.origin, "https://[fd7a::3]:7443")
    }

    func testRefusesOtherLinks() {
        let cases: [(String, PairingLink.ParseError)] = [
            ("not a url", .notAURL),
            ("http://192.168.1.20:7443/pair#t=tok&fp=\(hex)", .notHTTPS),
            ("https://user:pw@host/pair#t=tok&fp=\(hex)", .notHTTPS),
            ("https://host:7443/other#t=tok&fp=\(hex)", .notThePairingPage),
            ("https://host:7443/pair#fp=\(hex)", .missingToken),
            ("https://host:7443/pair#t=t%2Fo&fp=\(hex)", .missingToken),
            ("https://host:7443/pair#t=tok", .missingFingerprint),
            ("https://host:7443/pair#t=tok&fp=ABCD", .missingFingerprint),
        ]
        for (text, expected) in cases {
            XCTAssertThrowsError(try PairingLink(text), text) { error in
                XCTAssertEqual(error as? PairingLink.ParseError, expected, text)
            }
        }
    }

    func testFingerprintMatchesTheCertificateInTheChain() throws {
        let leaf = Data("leaf certificate".utf8)
        let ca = Data("ca certificate".utf8)
        let pinned = Fingerprint(certificate: ca)
        XCTAssertEqual(pinned.match(in: [leaf, ca]), 1)
        XCTAssertNil(pinned.match(in: [leaf]))
        XCTAssertNil(Fingerprint(certificate: leaf).match(in: [ca]))

        let digest = SHA256.hash(data: ca).map { String(format: "%02X", $0) }.joined()
        XCTAssertEqual(pinned.hex, digest)
        XCTAssertEqual(Fingerprint(pinned.description), pinned, "colon form reads back")
        XCTAssertEqual(Fingerprint(digest.lowercased()), pinned)
        XCTAssertNil(Fingerprint(String(repeating: "G", count: 64)))
    }

    func testBase64URLRoundTripsWithoutPadding() {
        let data = Data([0xfb, 0xff, 0xfe, 0x00, 0x01])
        let text = Base64URL.encode(data)
        XCTAssertEqual(text, "-__-AAE")
        XCTAssertEqual(Base64URL.decode(text), data)
        XCTAssertEqual(Base64URL.decode("-__-AAE="), data)
        XCTAssertNil(Base64URL.decode("+//+AAE"))
        XCTAssertNil(Base64URL.decode("a"))
        XCTAssertEqual(Data([0x0a, 0xff]).hexString, "0aff")
    }

    func testSessionMessageIsThePrefixThenTheNonceBytes() throws {
        let nonce = Data((0..<32).map { UInt8($0) })
        let message = try SessionMessage.bytes(nonce: Base64URL.encode(nonce))
        XCTAssertEqual(message.count, 21 + 32)
        XCTAssertEqual(Array(message.prefix(21)), Array("herdr-remote-session\u{0}".utf8))
        XCTAssertEqual(message.suffix(32), nonce)
        XCTAssertThrowsError(try SessionMessage.bytes(nonce: Base64URL.encode(Data(count: 31))))
    }

    func testDeviceKeyEncodingsAreWhatTheDaemonVerifies() throws {
        let key = P256.Signing.PrivateKey()
        let publicKey = try XCTUnwrap(Base64URL.decode(DeviceKeyEncoding.publicKey(key.publicKey)))
        XCTAssertEqual(publicKey.count, 65)
        XCTAssertEqual(publicKey.first, 0x04)

        let message = try SessionMessage.bytes(nonce: Base64URL.encode(Data(repeating: 7, count: 32)))
        let signature = try key.signature(for: message)
        let raw = try XCTUnwrap(Base64URL.decode(DeviceKeyEncoding.signature(signature)))
        XCTAssertEqual(raw.count, 64, "P1363 r || s, not DER")
        let parsed = try P256.Signing.ECDSASignature(rawRepresentation: raw)
        let verifier = try P256.Signing.PublicKey(x963Representation: publicKey)
        XCTAssertTrue(verifier.isValidSignature(parsed, for: message))
    }

    func testOriginsPutThePairingAddressFirst() {
        let origins = Computer.origins(
            pairedAt: "https://192.168.1.20:7443",
            served: ["https://100.64.0.3:7443", "https://192.168.1.20:7443"]
        )
        XCTAssertEqual(origins, ["https://192.168.1.20:7443", "https://100.64.0.3:7443"])
    }

    func testClaimStatusDecodesEachState() throws {
        let decode = { (json: String) in
            try JSONDecoder().decode(API.ClaimStatus.self, from: Data(json.utf8))
        }
        XCTAssertEqual(try decode(#"{"state":"claimed"}"#), .waiting)
        XCTAssertEqual(try decode(#"{"state":"denied"}"#), .denied)
        XCTAssertEqual(try decode(#"{"state":"expired"}"#), .expired)
        let colons = Array(repeating: "AB", count: 32).joined(separator: ":")
        let approved = try decode(
            #"{"state":"approved","device":"d1","origins":["https://a:1"],"fingerprint":"\#(colons)","computer":"studio"}"#
        )
        guard case let .approved(approval) = approved else { return XCTFail("\(approved)") }
        XCTAssertEqual(approval.device, "d1")
        XCTAssertEqual(approval.fingerprint.hex, hex)
        XCTAssertEqual(approval.computer, "studio")
    }

    func testRequestsUseTheDaemonsKeys() throws {
        let claim = try JSONSerialization.jsonObject(
            with: JSONEncoder().encode(API.ClaimRequest(token: "t", name: "iPhone", publicKey: "k"))
        ) as? [String: String]
        XCTAssertEqual(claim, ["token": "t", "name": "iPhone", "public_key": "k"])

        let registration = API.APNsRegistration(
            pushToStart: "aa",
            activities: [.init(pane: "w1-p3", token: "bb")]
        )
        let json = try JSONSerialization.jsonObject(with: JSONEncoder().encode(registration)) as? [String: Any]
        XCTAssertEqual(json?["push_to_start"] as? String, "aa")
        let activities = json?["activities"] as? [[String: String]]
        XCTAssertEqual(activities, [["pane": "w1-p3", "token": "bb"]])

        let none = try JSONSerialization.jsonObject(
            with: JSONEncoder().encode(API.APNsRegistration(pushToStart: nil, activities: []))
        ) as? [String: Any]
        XCTAssertNil(none?["push_to_start"])
    }
}
