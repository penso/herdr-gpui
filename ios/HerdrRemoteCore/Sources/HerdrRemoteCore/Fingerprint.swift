import CryptoKit
import Foundation

/// A certificate's SHA-256 fingerprint: 32 bytes, written as 64 uppercase
/// hex digits. The QR carries it without separators; the daemon shows it
/// with colons. Both read the same.
public struct Fingerprint: Hashable, Codable, Sendable, CustomStringConvertible {
    public let bytes: Data

    /// Reads hex with or without `:` separators, in either case.
    public init?(_ text: String) {
        let hex = text.replacingOccurrences(of: ":", with: "")
        guard hex.count == 64 else { return nil }
        var bytes = Data(capacity: 32)
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<next], radix: 16) else { return nil }
            bytes.append(byte)
            index = next
        }
        self.bytes = bytes
    }

    /// The fingerprint of a DER certificate.
    public init(certificate der: Data) {
        bytes = Data(SHA256.hash(data: der))
    }

    /// Uppercase hex without separators, as the QR carries it.
    public var hex: String {
        bytes.map { String(format: "%02X", $0) }.joined()
    }

    /// Uppercase hex pairs joined by colons, as Herdr UI shows it.
    public var description: String {
        bytes.map { String(format: "%02X", $0) }.joined(separator: ":")
    }

    /// The first certificate of `chain` (DER, leaf first) this fingerprint
    /// names, if any: the computer's CA, which the app then trusts alone.
    public func match(in chain: [Data]) -> Int? {
        chain.firstIndex { Fingerprint(certificate: $0) == self }
    }

    public init(from decoder: Decoder) throws {
        let text = try decoder.singleValueContainer().decode(String.self)
        guard let fingerprint = Fingerprint(text) else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath, debugDescription: "not a SHA-256 fingerprint")
            )
        }
        self = fingerprint
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(hex)
    }
}
