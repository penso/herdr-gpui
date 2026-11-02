import Foundation
import Testing
@testable import HerdrRemoteCore

/// The Workspaces tab lists a computer's workspaces by repository, as
/// Herdr UI's sidebar does.
struct RepositoriesTests {
    @Test func decodesLoadAndWorktrees() throws {
        guard case let .home(home) = try WireTests.event("home") else { Issue.record("not home"); return }
        #expect(home.load == Home.Load(cpu: 47, memory: 67))
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

    static func workspace(_ id: String, worktree: Home.Worktree? = nil) -> Home.Workspace {
        Home.Workspace(id: id, label: id, branch: nil, cwd: "/\(id)", worktree: worktree, pullRequest: nil, chats: [], terminals: [])
    }
}
