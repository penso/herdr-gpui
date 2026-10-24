import Foundation

/// base64url without padding (RFC 4648 section 5), the encoding the daemon
/// uses for keys, nonces, signatures, and tokens.
public enum Base64URL {
    public static func encode(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    /// Decodes with or without padding; `nil` for anything else.
    public static func decode(_ text: String) -> Data? {
        guard text.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || "-_=".contains($0)) })
        else { return nil }
        var base64 = text
            .replacingOccurrences(of: "=", with: "")
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        switch base64.count % 4 {
        case 0: break
        case 2: base64 += "=="
        case 3: base64 += "="
        default: return nil
        }
        return Data(base64Encoded: base64)
    }
}

extension Data {
    /// Lowercase hex, the form APNs tokens take on the wire.
    public var hexString: String {
        map { String(format: "%02x", $0) }.joined()
    }
}
