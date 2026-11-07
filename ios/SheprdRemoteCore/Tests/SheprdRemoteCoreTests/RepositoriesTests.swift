import Foundation
import Testing
@testable import SheprdRemoteCore

/// The Workspaces tab lists a computer's workspaces by repository, as
/// Sheprd's sidebar does.
struct RepositoriesTests {
    @Test func decodesLoadAndWorktrees() throws {
        guard case let .home(home) = try WireTests.event("home") else { Issue.record("not home"); return }
        #expect(home.load == Home.Load(cores: 10, cpu: 47, load5Tenths: 42, memory: 67))
        #expect(home.working == 2)
        let agents = home.workspaces.flatMap(\.terminals).compactMap(\.agent)
        #expect(agents.contains { $0.changedMs == 1_770_000_000_000 })
        #expect(home.workspaces[0].worktree == Home.Worktree(key: "/work/.git", label: "work", linked: false))
        #expect(home.workspaces[1].worktree == nil)
        #expect(home.workspaces[2].worktree?.linked == true)
    }

    @Test func groupsTheFixturesWorktreesUnderTheirRepository() throws {
        guard case let .home(home) = try WireTests.event("home") else { Issue.record("not home"); return }
        let repositories = home.repositories
        #expect(repositories.map(\.label) == ["work", "moltis"])
        #expect(repositories[0].workspaces.map(\.id) == ["w1", "w3"])
        #expect(repositories[1].id == "workspace/w2")
    }

    @Test func listsTheOwnCheckoutBeforeLinkedWorktreesOpenedEarlier() {
        let home = Self.home([
            Self.workspace("a", worktree: .init(key: "k", label: "repo", linked: true)),
            Self.workspace("b"),
            Self.workspace("c", worktree: .init(key: "k", label: "repo", linked: false)),
            Self.workspace("d", worktree: .init(key: "k", label: "repo", linked: true)),
        ])
        #expect(home.repositories.map(\.label) == ["repo", "b"])
        #expect(home.repositories[0].workspaces.map(\.id) == ["c", "a", "d"])
    }

    @Test func decodesAHomeFromAnOlderComputer() throws {
        let json = Data(#"{"computer":"c","fingerprint":"f","connected":true,"waiting":0,"workspaces":[{"id":"w","label":"l","cwd":"/","chats":[],"terminals":[]}]}"#.utf8)
        let home = try JSONDecoder().decode(Home.self, from: json)
        #expect(home.load == nil)
        #expect(home.repositories.map(\.label) == ["l"])
        #expect(home.workspaces[0].session == nil)
        #expect(home.sessionGroups == [SessionGroup(name: nil, repositories: home.repositories)])
    }

    @Test func groupsTheFixturesWorkspacesBySession() throws {
        guard case let .home(home) = try WireTests.event("home") else { Issue.record("not home"); return }
        #expect(home.workspaces.map(\.session) == ["default", "pilots", "default"])
        let groups = home.sessionGroups
        #expect(groups.map(\.name) == ["default", "pilots"])
        #expect(groups[0].repositories.map(\.label) == ["work"])
        #expect(groups[0].repositories[0].workspaces.map(\.id) == ["w1", "w3"])
        #expect(groups[1].repositories.map(\.label) == ["moltis"])
    }

    @Test func keepsARepositoryInTwoSessionsApartAndHerdr2First() {
        let worktree = Home.Worktree(key: "k", label: "repo", linked: false)
        let home = Self.home([
            Self.workspace("a", worktree: worktree, session: "coding"),
            Self.workspace("b"),
            Self.workspace("c", worktree: worktree, session: "pilots"),
        ])
        let groups = home.sessionGroups
        #expect(groups.map(\.name) == [nil, "coding", "pilots"])
        #expect(groups.map { $0.repositories.flatMap(\.workspaces).map(\.id) } == [["b"], ["a"], ["c"]])
    }

    @Test func aNewWorktreeStartsInItsCheckoutsSessionUnderNoHeader() {
        // Only a followed session lists workspaces, so no header names it.
        let worktree = Home.Worktree(key: "k", label: "repo", linked: false)
        let home = Self.home([
            Self.workspace("a", worktree: .init(key: "k", label: "repo", linked: true), session: "work"),
            Self.workspace("b", worktree: worktree, session: "work"),
        ])
        #expect(home.sessionGroups.map(\.name) == [nil])
        let checkout = home.sessionGroups[0].repositories[0].ownCheckout
        #expect(checkout?.id == "b")
        #expect(checkout?.session == "work")
    }

    @Test func listsOneNamedSessionAsBeforeSessionsWereNamed() {
        let home = Self.home([
            Self.workspace("a", session: "default"),
            Self.workspace("b"),
            Self.workspace("c", session: "default"),
        ])
        #expect(home.sessionGroups == [SessionGroup(name: nil, repositories: home.repositories)])
    }

    @Test func takesTheMostPressingStatus() throws {
        guard case let .home(home) = try WireTests.event("home") else { Issue.record("not home"); return }
        // A working chat beats a terminal agent that is done.
        #expect(home.workspaces[0].status == .working)
        #expect(home.workspaces[0].asking)
        #expect(home.workspaces[1].status == nil)
        #expect(home.workspaces[1].anyPane == nil)
        #expect(home.workspaces[0].anyPane == "p2")
    }

    @Test func tellsALoadChangeFromAWorkspaceChange() throws {
        guard case let .home(home) = try WireTests.event("home") else { Issue.record("not home"); return }
        var busier = home
        busier.load = Home.Load(cpu: 90, memory: 67)
        #expect(home.sameApartFromLoad(busier))
        busier.workspaces.removeLast()
        #expect(!home.sameApartFromLoad(busier))
    }

    static func home(_ workspaces: [Home.Workspace]) -> Home {
        Home(computer: "c", fingerprint: "f", connected: true, waiting: 0, workspaces: workspaces)
    }

    static func workspace(_ id: String, worktree: Home.Worktree? = nil, session: String? = nil) -> Home.Workspace {
        Home.Workspace(id: id, label: id, branch: nil, cwd: "/\(id)", worktree: worktree, session: session, pullRequest: nil, chats: [], terminals: [])
    }
}
