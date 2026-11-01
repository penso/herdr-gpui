import Foundation

/// The app icon's badge across paired computers. Each computer counts only
/// its own waiting questions, so a push carries that count and the badge
/// shows the sum of the last one from each.
public struct BadgeCounts: Codable, Hashable, Sendable {
    /// Waiting questions by computer (its CA fingerprint in hex).
    public private(set) var counts: [String: Int] = [:]

    public init(counts: [String: Int] = [:]) {
        self.counts = counts.filter { $0.value > 0 }
    }

    public var total: Int { counts.values.reduce(0, +) }

    /// Takes `count` as `computer`'s; zero drops it.
    public mutating func set(_ count: Int, for computer: String) {
        counts[computer.uppercased()] = count > 0 ? count : nil
    }

    /// Keeps only the computers still paired, so an unpaired one's count
    /// does not stay on the badge.
    public mutating func keep(only computers: Set<String>) {
        let kept = Set(computers.map { $0.uppercased() })
        counts = counts.filter { kept.contains($0.key) }
    }

    private static let key = "badgeCounts"

    /// The counts the app and its extensions share.
    public static func load(from defaults: UserDefaults?) -> BadgeCounts {
        guard let data = defaults?.data(forKey: key) else { return BadgeCounts() }
        return (try? JSONDecoder().decode(BadgeCounts.self, from: data)) ?? BadgeCounts()
    }

    public func save(to defaults: UserDefaults?) {
        defaults?.set(try? JSONEncoder().encode(self), forKey: Self.key)
    }

    /// Records `count` for `computer` in `defaults` and answers the badge.
    public static func update(_ count: Int, for computer: String, in defaults: UserDefaults?) -> Int {
        var counts = load(from: defaults)
        counts.set(count, for: computer)
        counts.save(to: defaults)
        return counts.total
    }
}
