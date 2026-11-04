import Testing
@testable import SheprdRemoteCore

/// New workspaces go to the computer with the most room, as the desktop's
/// dispatch ranks hosts.
struct LeastBusyTests {
    private static func candidate(
        _ id: String, cpu: Int? = nil, cores: Int? = 10, load5: Int? = nil, memory: Int = 10,
        working: Int = 0, connected: Bool = true, repository: String? = nil
    ) -> Home.Candidate {
        let chats = (0..<working).map {
            Home.ChatRow(pane: "p\($0)", tab: "t\($0)", title: "", agent: nil, status: .working, summary: nil)
        }
        let worktree = repository.map { Home.Worktree(key: "/\($0)/.git", label: $0, linked: false) }
        let workspace = Home.Workspace(id: "w", label: "w", branch: nil, cwd: "/\(id)/w", worktree: worktree, pullRequest: nil, chats: chats, terminals: [])
        let load = cpu.map { Home.Load(cores: cores, cpu: $0, load5Tenths: load5, memory: memory) }
        let home = Home(computer: id, fingerprint: "", connected: connected, waiting: 0, load: load, workspaces: [workspace])
        return Home.Candidate(id: id, label: id, home: home)
    }

    private static func order(_ candidates: [Home.Candidate], current: String? = nil, repository: String? = nil) -> [String] {
        Home.leastBusy(candidates, current: current, repository: repository).map(\.id)
    }

    /// The values `spare_cores_prefer_the_load_average_over_a_momentary_share`
    /// checks on the desktop's side: the average wins, and spare stops at 0.
    @Test func spareCoresPreferTheLoadAverage() {
        #expect(Home.Load(cores: 8, cpu: 100, load5Tenths: 20, memory: 0).spareCores == 6)
        #expect(Home.Load(cores: 8, cpu: 50, load5Tenths: nil, memory: 0).spareCores == 4)
        #expect(Home.Load(cores: 4, cpu: 0, load5Tenths: 90, memory: 0).spareCores == 0)
    }

    @Test func moreCoresBeatALowerShare() {
        // 30% of 32 cores leaves more room than 10% of 4.
        let big = Self.candidate("big", cpu: 30, cores: 32)
        #expect(Self.order([Self.candidate("small", cpu: 10, cores: 4), big]) == ["big", "small"])
    }

    @Test func anOlderComputerWithoutACoreCountStillRanks() {
        #expect(Self.order([Self.candidate("a", cpu: 80, cores: nil), Self.candidate("b", cpu: 20, cores: nil)]) == ["b", "a"])
    }

    @Test func workingAgentsCountAgainstAComputer() {
        let busy = Self.candidate("a", cpu: 10, working: 4)
        #expect(Self.order([busy, Self.candidate("b", cpu: 20)]) == ["b", "a"])
    }

    @Test func theComputersOwnCountBeatsWhatItsTabsShow() {
        var split = Self.candidate("a", cpu: 10)
        split.home.working = 8
        #expect(Self.order([split, Self.candidate("b", cpu: 20)]) == ["b", "a"])
    }

    @Test func tightMemoryHalvesTheSpareCores() {
        let tight = Self.candidate("a", cpu: 10, memory: 90)
        #expect(Self.order([tight, Self.candidate("b", cpu: 50)]) == ["b", "a"])
    }

    @Test func anUnreadLoadComesAfterAReadOne() {
        #expect(Self.order([Self.candidate("a"), Self.candidate("b", cpu: 90)]) == ["b", "a"])
    }

    @Test func aComputerWhoseDaemonIsDownIsLeftOut() {
        #expect(Self.order([Self.candidate("a", cpu: 0, connected: false), Self.candidate("b", cpu: 90)]) == ["b"])
    }

    @Test func aCloseCallStaysPutAndATieGoesByLabel() {
        let close = [Self.candidate("a", cpu: 21), Self.candidate("b", cpu: 20)]
        #expect(Self.order(close) == ["b", "a"])
        #expect(Self.order(close, current: "a") == ["a", "b"])
        #expect(Self.order([Self.candidate("b", cpu: 20), Self.candidate("a", cpu: 20)]) == ["a", "b"])
    }

    @Test func aComputerWithTheRepositoryBeatsAnIdlerOneWithout() {
        let idle = Self.candidate("idle", cpu: 0, repository: "other")
        let busy = Self.candidate("busy", cpu: 90, repository: "sheprd")
        #expect(Self.order([idle, busy], repository: "sheprd") == ["busy", "idle"])
        // Among those without it, or with none named, room decides.
        #expect(Self.order([busy, idle], repository: "moltis") == ["idle", "busy"])
        #expect(Self.order([busy, idle]) == ["idle", "busy"])
    }

    @Test func findsARepositorysOwnCheckoutBeforeALinkedOne() {
        func workspace(_ cwd: String, linked: Bool) -> Home.Workspace {
            Home.Workspace(id: cwd, label: cwd, branch: nil, cwd: cwd, worktree: .init(key: "k", label: "repo", linked: linked), pullRequest: nil, chats: [], terminals: [])
        }
        let home = Home(computer: "c", fingerprint: "", connected: true, waiting: 0, workspaces: [workspace("/linked", linked: true), workspace("/own", linked: false)])
        #expect(home.checkout(ofRepository: "repo") == "/own")
        #expect(home.checkout(ofRepository: "other") == nil)
    }
}
