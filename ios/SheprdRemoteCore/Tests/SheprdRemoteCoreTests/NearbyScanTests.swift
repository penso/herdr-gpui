import SheprdRemoteCore
import Testing

struct NearbyScanTests {
    private func looking(likely: Bool = false, exchanging: Bool = false, foreground: Bool = false, allowed: Bool = false) -> Bool {
        NearbyScan(pairingLikely: likely, exchanging: exchanging, foreground: foreground, allowed: allowed).looking
    }

    @Test func inFrontItLooksOnceAllowed() {
        #expect(looking(foreground: true, allowed: true))
    }

    /// Looking would raise the permission prompt out of nowhere.
    @Test func inFrontItDoesNotAskForPermissionUnprompted() {
        #expect(!looking(foreground: true, allowed: false))
    }

    /// A card left open in the background is not an exchange.
    @Test func inTheBackgroundItStopsWhenNothingIsUnderWay() {
        #expect(!looking(foreground: false, allowed: true))
    }

    @Test func anExchangeUnderWayKeepsGoingInTheBackground() {
        #expect(looking(exchanging: true, foreground: false, allowed: true))
    }

    @Test func likelyPairingLooksEvenBeforePermission() {
        #expect(looking(likely: true))
    }
}
