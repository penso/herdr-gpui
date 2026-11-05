import Testing
@testable import SheprdRemoteCore

@Suite struct TermShiftTests {
    @Test func heldShiftReachesArrowsAndTab() {
        #expect(TermInput.pressed("up", held: .shift) == "shift+up")
        #expect(TermInput.pressed("down", held: .shift) == "shift+down")
        #expect(TermInput.pressed("tab", held: .shift) == "shift+tab")
    }

    @Test func heldShiftLeavesOtherKeysAlone() {
        #expect(TermInput.pressed("esc", held: .shift) == "esc")
        #expect(TermInput.pressed("backspace", held: .shift) == "backspace")
        #expect(TermInput.pressed("slash", held: .shift) == "slash")
    }

    /// ^C must still interrupt while shift is latched.
    @Test func heldShiftLeavesCombinationsAlone() {
        #expect(TermInput.pressed("ctrl+c", held: .shift) == "ctrl+c")
    }

    /// A hardware Control or Option arrow arrives as a combination.
    @Test func heldShiftReachesCombinedArrows() {
        #expect(TermInput.pressed("ctrl+up", held: .shift) == "ctrl+shift+up")
        #expect(TermInput.pressed("alt+left", held: .shift) == "alt+shift+left")
    }

    @Test func aCombinationNamingShiftKeepsIt() {
        #expect(TermInput.pressed("shift+tab") == "shift+tab")
        #expect(TermInput.pressed("ctrl+shift+up") == "ctrl+shift+up")
    }

    @Test func otherHeldModifiersStillApply() {
        #expect(TermInput.pressed("up", held: .ctrl) == "ctrl+up")
        #expect(TermInput.pressed("c", held: .ctrl) == "ctrl+c")
        #expect(TermInput.pressed("up", held: [.ctrl, .shift]) == "ctrl+shift+up")
    }

    @Test func onlyShiftStaysHeldAfterAKey() {
        #expect(TermInput.Modifiers([.ctrl, .alt, .shift]).afterKey == .shift)
        #expect(TermInput.Modifiers([.ctrl, .alt]).afterKey == [])
    }
}
