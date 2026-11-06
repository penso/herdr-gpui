import Foundation

/// What a failed call says about the address it used, and how logs name it.
public enum PathFailure {
    /// Path segments followed by an identifier, which logs leave out: a
    /// pairing claim's is a secret, and the daemon's log keeps none either.
    private static let identified: Set<String> = ["chat", "claim", "hosts", "term", "workspaces"]

    /// Whether nothing came back from the address. An answer of any kind,
    /// a refused certificate, or a cancelled call says the path still works.
    public static func isDead(_ error: Error) -> Bool {
        switch (error as? URLError)?.code {
        case .cannotConnectToHost, .cannotFindHost, .dnsLookupFailed,
             .networkConnectionLost, .notConnectedToInternet, .timedOut:
            true
        default:
            false
        }
    }

    /// `url`'s path with identifiers replaced, and never its query, which
    /// carries what the person searched for.
    public static func route(_ url: URL?) -> String {
        guard let url else { return "(no url)" }
        var previous = ""
        let segments = url.path.split(separator: "/").map { segment in
            defer { previous = String(segment) }
            return identified.contains(previous) ? "{id}" : String(segment)
        }
        return "/" + segments.joined(separator: "/")
    }

    /// The error's text with its code, which stays the same in every
    /// language, and how long the call had been going.
    public static func summary(_ error: Error, after elapsed: Duration) -> String {
        let code = error as NSError
        return "\(code.localizedDescription) (\(code.domain) \(code.code)) after \(seconds(elapsed))"
    }

    /// `elapsed` to a tenth of a second, as logs show it.
    public static func seconds(_ elapsed: Duration) -> String {
        let parts = elapsed.components
        let value = Double(parts.seconds) + Double(parts.attoseconds) / 1e18
        return value.formatted(.number.precision(.fractionLength(1)).locale(Locale(identifier: "en_US_POSIX"))) + "s"
    }
}
