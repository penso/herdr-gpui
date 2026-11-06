import Testing
@testable import SheprdRemoteCore

@Suite struct AgentSelectionTests {
    private let codex = AgentChoice(id: "codex", name: "Codex")
    private let pi = AgentChoice(id: "pi", name: "Pi")

    @Test func anUnavailableDefaultSelectsAnOfferedAgent() {
        let options = NewOptions(agents: [codex], recent: [], roots: [])
        let selected = options.agent(preferring: "claude")
        #expect(selected == codex)
    }

    @Test func anAvailablePresetIsPreserved() {
        let options = NewOptions(agents: [codex, pi], recent: [], roots: [])
        #expect(options.agent(preferring: "pi") == pi)
    }

    @Test func anEmptyListHasNoSelection() {
        let options = NewOptions(agents: [], recent: [], roots: [])
        #expect(options.agent(preferring: "claude") == nil)
    }
}
