import Foundation

/// Failures waiting to reach a computer's log, oldest first. Bounded, as a
/// computer may stay away; a line stays until the computer accepts it.
public struct UnreportedFailures: Equatable, Sendable {
    /// The daemon takes ten reports at once from an address.
    public static let limit = 8

    public private(set) var lines: [String] = []

    public init() {}

    public mutating func add(_ line: String) {
        lines = Array((lines + [line]).suffix(Self.limit))
    }

    /// The computer took `line`. Newer failures may have pushed it out
    /// while it was on its way, which leaves nothing to remove.
    public mutating func accepted(_ line: String) {
        if let index = lines.firstIndex(of: line) { lines.remove(at: index) }
    }
}

/// What a computer said when it refused a call.
public enum Refusal {
    /// The daemon's `{"error": "..."}` as a sentence for the screen, else
    /// the status with whatever came back.
    public static func text(status: Int, body: String) -> String {
        struct Answer: Decodable { let error: String }
        guard let answer = try? JSONDecoder().decode(Answer.self, from: Data(body.utf8)), !answer.error.isEmpty else {
            return "The computer answered \(status): \(body)"
        }
        return answer.error
    }
}

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
