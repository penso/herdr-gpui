import Foundation
@testable import HerdrRemoteCore
import Testing

struct BadgeCountsTests {
    @Test func aComputerReplacesOnlyItsOwnCount() {
        var counts = BadgeCounts()
        counts.set(2, for: "AA")
        counts.set(1, for: "bb")
        #expect(counts.total == 3)
        counts.set(0, for: "AA")
        #expect(counts.total == 1)
        counts.set(4, for: "BB")
        #expect(counts.total == 4)
    }

    @Test func anUnpairedComputerLeavesTheBadge() {
        var counts = BadgeCounts(counts: ["AA": 2, "BB": 3])
        counts.keep(only: ["bb"])
        #expect(counts.total == 3)
    }

    @Test func countsSurviveAProcessThroughTheSharedDefaults() throws {
        let defaults = try #require(UserDefaults(suiteName: "badge-tests-\(UUID().uuidString)"))
        #expect(BadgeCounts.update(2, for: "AA", in: defaults) == 2)
        #expect(BadgeCounts.update(1, for: "BB", in: defaults) == 3)
        #expect(BadgeCounts.update(0, for: "AA", in: defaults) == 1)
        #expect(BadgeCounts.load(from: defaults).counts == ["BB": 1])
    }
}
