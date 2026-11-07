import Foundation

/// When each Bluetooth peripheral last advertised Sheprd's pairing service.
/// A computer advertises it only while its pairing dialog is open, so one
/// not heard from for `lifetime` has closed it: its offer leaves the list,
/// and the next advertisement lists it again.
public struct NearbySightings: Sendable {
    /// Long enough to ride out a few missed advertisements.
    public static let lifetime: TimeInterval = 8

    private var last: [UUID: Date] = [:]

    public init() {}

    /// Records an advertisement from `id` at `now`; true for one not
    /// already known.
    public mutating func saw(_ id: UUID, at now: Date) -> Bool {
        last.updateValue(now, forKey: id) == nil
    }

    /// Forgets and returns the peripherals unheard since `lifetime` before
    /// `now`, except those in `keeping` (a pairing in progress).
    public mutating func expire(at now: Date, keeping: Set<UUID> = []) -> [UUID] {
        let stale = last.filter { id, seen in
            !keeping.contains(id) && now.timeIntervalSince(seen) > Self.lifetime
        }.map(\.key)
        for id in stale { last[id] = nil }
        return stale
    }

    public mutating func forget(_ id: UUID) {
        last[id] = nil
    }

    public mutating func removeAll() {
        last = [:]
    }
}
