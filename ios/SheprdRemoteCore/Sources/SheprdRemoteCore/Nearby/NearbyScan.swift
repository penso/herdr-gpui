/// When to look for computers over Bluetooth. Only an exchange under way
/// keeps it going in the background, where the app may run for a long time:
/// a card merely left open is not pairing, and offers come back on return.
public struct NearbyScan: Hashable, Sendable {
    /// The pair sheet is open, or no computer is paired: pairing is likely,
    /// so this is when the permission prompt may come.
    public var pairingLikely: Bool
    /// A pairing has started and has not finished.
    public var exchanging: Bool
    /// The app is in front.
    public var foreground: Bool
    /// The person allowed Bluetooth, so looking prompts for nothing.
    public var allowed: Bool

    public init(pairingLikely: Bool, exchanging: Bool, foreground: Bool, allowed: Bool) {
        self.pairingLikely = pairingLikely
        self.exchanging = exchanging
        self.foreground = foreground
        self.allowed = allowed
    }

    public var looking: Bool {
        pairingLikely || exchanging || (foreground && allowed)
    }
}
