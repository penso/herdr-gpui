import SwiftUI

// Placeholder data for the Spaces mockups, shaped like the desktop
// sidebar: a computer with its load, repositories, and their worktrees.

enum SpaceMark {
    case idle, working, unread, needsYou

    @ViewBuilder var view: some View {
        switch self {
        case .idle: Circle().strokeBorder(.green, lineWidth: 1.5).frame(width: 10, height: 10)
        case .working:
            Circle().trim(from: 0, to: 0.7).stroke(.orange, style: StrokeStyle(lineWidth: 1.8, lineCap: .round))
                .rotationEffect(.degrees(-90)).frame(width: 9, height: 9)
        case .unread: Circle().fill(.blue).frame(width: 10, height: 10)
        case .needsYou: Circle().fill(.orange).frame(width: 10, height: 10)
        }
    }
}

enum PRTone {
    case open, failing, merged, review

    var color: Color {
        switch self {
        case .open: .green
        case .failing: .red
        case .merged: .purple
        case .review: Color(red: 0.75, green: 0.55, blue: 0.2)
        }
    }
}

struct SpacePR {
    var number: Int
    var tone: PRTone
}

struct Space: Identifiable {
    let id = UUID()
    var name: String
    var branch: String
    var mark: SpaceMark = .idle
    var pr: SpacePR? = nil
    var note = false
    var agents = 0
    var terminals = 1
}

struct Repo: Identifiable {
    let id = UUID()
    var name: String
    var avatar: Color
    var main: Space
    var worktrees: [Space] = []
}

struct Machine: Identifiable {
    let id = UUID()
    var name: String
    var symbol: String
    var via: String
    var online: Bool
    var cpu: Double
    var memory: Double
    var gpu: Double?
    var repos: [Repo]

    var spaceCount: Int { repos.reduce(0) { $0 + 1 + $1.worktrees.count } }
    var needsYou: Int { repos.flatMap { [$0.main] + $0.worktrees }.filter { $0.mark == .needsYou }.count }
}

/// A tab inside a workspace: an agent chat or a terminal.
struct SpaceTab: Identifiable {
    let id = UUID()
    var title: String
    var agent: Agent
    var status: Status
    var detail: String
    var elapsed: String? = nil
    var preview: [String] = []
}

enum SpaceSample {
    static let machines: [Machine] = [
        Machine(name: "m4max", symbol: "laptopcomputer", via: "This network", online: true,
                cpu: 0.47, memory: 0.67, gpu: 0.12, repos: [
            Repo(name: "agent-launcher", avatar: .green, main: Space(name: "agent-launcher", branch: "main")),
            Repo(name: "moltis", avatar: .orange, main: Space(name: "moltis", branch: "main", note: true), worktrees: [
                Space(name: "1255-bug-agentend-message", branch: "agent/1255", pr: SpacePR(number: 1267, tone: .open), agents: 1),
                Space(name: "1254-feature-include-a-stable", branch: "agent/1254", note: true),
                Space(name: "moltis-064r-fix-oauth-pkce", branch: "fix/oauth", pr: SpacePR(number: 1269, tone: .failing)),
                Space(name: "Moltis UI", branch: "moltis-ui", note: true),
            ]),
            Repo(name: "butler", avatar: .green, main: Space(name: "butler", branch: "main"), worktrees: [
                Space(name: "priority-5 web & polish", branch: "priority-5", pr: SpacePR(number: 22, tone: .open)),
            ]),
            Repo(name: "herdr-gpui", avatar: .green, main: Space(name: "herdr-gpui", branch: "main"), worktrees: [
                Space(name: "browser agent", branch: "worktree/browser", note: true),
                Space(name: "Phone push notifications", branch: "push", pr: SpacePR(number: 256, tone: .failing)),
                Space(name: "i18n 5 platform", branch: "i18n-5", mark: .unread, pr: SpacePR(number: 908, tone: .failing), agents: 1),
                Space(name: "mobile app preview", branch: "worktree/mobile-app-preview", mark: .needsYou,
                      pr: SpacePR(number: 905, tone: .review), note: true, agents: 2, terminals: 2),
                Space(name: "renaming", branch: "renaming", mark: .unread, pr: SpacePR(number: 906, tone: .open), agents: 1),
                Space(name: "herdr app", branch: "herdr-app", mark: .working, pr: SpacePR(number: 907, tone: .failing), agents: 1),
            ]),
        ]),
        Machine(name: "build-01", symbol: "server.rack", via: "SSH through m4max", online: true,
                cpu: 0.82, memory: 0.31, gpu: 0.64, repos: [
            Repo(name: "release", avatar: .blue, main: Space(name: "release", branch: "v0.42", mark: .working, agents: 1)),
        ]),
    ]

    static var local: Machine { machines[0] }

    static let detail = Space(name: "mobile app preview", branch: "worktree/mobile-app-preview", mark: .needsYou,
                              pr: SpacePR(number: 905, tone: .review), note: true, agents: 2, terminals: 2)

    static let tabs: [SpaceTab] = [
        SpaceTab(title: "Spaces tab mockups", agent: .claude, status: .needsYou,
                 detail: "Allow `xcodebuild -project HerdrMockups…`?",
                 preview: ["Write Sources/Spaces.swift", "Run ./render.sh dark spaces"]),
        SpaceTab(title: "iOS reconnect backoff", agent: .codex, status: .working, detail: "Running swift test",
                 elapsed: "2:41", preview: ["Edit LiveSocket.swift +31 −8", "Run swift test"]),
        SpaceTab(title: "zsh", agent: .terminal, status: .idle, detail: "~/…/worktree-mobile-app-preview",
                 preview: ["$ just ci", "test result: ok. 412 passed"]),
        SpaceTab(title: "just run", agent: .terminal, status: .idle, detail: "herdr-gpui · release",
                 preview: ["Finished `release` profile", "Running target/release/herdr-gpui"]),
    ]
}

/// Desktop-style CPU, MEM, and GPU readouts.
struct LoadReadout: View {
    var label: String
    var value: Double
    var mono = false

    var body: some View {
        HStack(spacing: 5) {
            Text(label).font(mono ? .caption.monospaced() : .caption2.weight(.semibold)).foregroundStyle(.secondary)
            Capsule().fill(.quaternary).frame(width: 34, height: 5)
                .overlay(alignment: .leading) {
                    Capsule().fill(value > 0.75 ? Color.orange : Color.secondary).frame(width: 34 * value, height: 5)
                }
            Text("\(Int(value * 100))%").font(mono ? .caption.monospaced() : .caption2.monospacedDigit())
                .foregroundStyle(.secondary)
        }
    }
}

struct MachineLoad: View {
    var machine: Machine
    var mono = false

    var body: some View {
        HStack(spacing: 12) {
            LoadReadout(label: "CPU", value: machine.cpu, mono: mono)
            LoadReadout(label: "MEM", value: machine.memory, mono: mono)
            if let gpu = machine.gpu { LoadReadout(label: "GPU", value: gpu, mono: mono) }
        }
    }
}

struct RepoAvatar: View {
    var repo: Repo
    var size: CGFloat = 22

    var body: some View {
        Text(String(repo.name.prefix(1)).uppercased())
            .font(.system(size: size * 0.5, weight: .bold, design: .rounded))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(repo.avatar.gradient, in: .rect(cornerRadius: size * 0.3))
    }
}

struct PRBadge: View {
    var pr: SpacePR
    var mono = false

    var body: some View {
        Text(verbatim: "#\(pr.number)")
            .font(mono ? .footnote.monospaced() : .caption.weight(.semibold).monospacedDigit())
            .foregroundStyle(pr.tone.color)
    }
}

/// The tab bar the app has today, with Spaces in place of Computers.
struct SpacesTabs<Content: View>: View {
    var title = "Spaces"
    @ViewBuilder var content: Content

    var body: some View {
        TabView(selection: .constant(1)) {
            Tab("Home", systemImage: "house", value: 0) { Text("Home") }
            Tab(title, systemImage: "square.stack.3d.up", value: 1) { content }
            Tab("Settings", systemImage: "gearshape", value: 2) { Text("Settings") }
        }
    }
}
