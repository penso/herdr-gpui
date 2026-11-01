import SwiftUI

// Placeholder data shared by every mockup. Names and numbers are made up.

enum Agent {
    case claude, codex, gemini, terminal

    var symbol: String {
        switch self {
        case .claude: "sparkle"
        case .codex: "hexagon.fill"
        case .gemini: "diamond.fill"
        case .terminal: "apple.terminal"
        }
    }

    var tint: Color {
        switch self {
        case .claude: Color(red: 0.85, green: 0.47, blue: 0.34)
        case .codex: Color(white: 0.55)
        case .gemini: Color(red: 0.36, green: 0.52, blue: 0.96)
        case .terminal: Color(white: 0.45)
        }
    }

    var name: String {
        switch self {
        case .claude: "Claude"
        case .codex: "Codex"
        case .gemini: "Gemini"
        case .terminal: "Terminal"
        }
    }
}

enum Status {
    case working, needsYou, unread, idle, done
}

struct Chat: Identifiable {
    let id = UUID()
    var title: String
    var agent: Agent
    var status: Status
    var detail: String
    var elapsed: String? = nil
    var progress: Double? = nil
}

struct Workspace: Identifiable {
    let id = UUID()
    var name: String
    var branch: String
    var chats: [Chat]
}

struct Computer: Identifiable {
    let id = UUID()
    var name: String
    var symbol: String
    var via: String
    var workspaces: [Workspace]

    var needsYou: Int { workspaces.flatMap(\.chats).filter { $0.status == .needsYou }.count }
    var working: Int { workspaces.flatMap(\.chats).filter { $0.status == .working }.count }
}

struct Question: Identifiable {
    let id = UUID()
    var computer: String
    var workspace: String
    var chat: String
    var agent: Agent
    var prompt: String
    var options: [String]
    var isPermission: Bool
}

enum Sample {
    static let computers: [Computer] = [
        Computer(
            name: "MacBook Pro", symbol: "laptopcomputer", via: "Wi-Fi · 192.168.1.20",
            workspaces: [
                Workspace(name: "herdr-gpui", branch: "mobile-app", chats: [
                    Chat(title: "Equalize splits command", agent: .claude, status: .working,
                         detail: "Running just ci", elapsed: "4:12", progress: 0.6),
                    Chat(title: "Fix mise discovery on SSH", agent: .codex, status: .needsYou,
                         detail: "Allow `git push origin fix/mise`?"),
                    Chat(title: "zsh — just test-gui", agent: .terminal, status: .idle,
                         detail: "~/code/herdr-gpui"),
                ]),
                Workspace(name: "moltis", branch: "main", chats: [
                    Chat(title: "Voice pipeline latency", agent: .claude, status: .unread,
                         detail: "Done in 6m · $1.12"),
                    Chat(title: "GraphQL schema cleanup", agent: .gemini, status: .idle,
                         detail: "Yesterday"),
                ]),
            ]),
        Computer(
            name: "studio", symbol: "macstudio", via: "Tailscale · 100.88.4.2",
            workspaces: [
                Workspace(name: "site", branch: "redesign", chats: [
                    Chat(title: "Hero animation", agent: .claude, status: .needsYou,
                         detail: "Which layout should the hero use?"),
                    Chat(title: "Lighthouse pass", agent: .codex, status: .working,
                         detail: "Editing styles.css", elapsed: "1:03", progress: 0.3),
                ]),
            ]),
        Computer(
            name: "build-01", symbol: "server.rack", via: "SSH through MacBook Pro",
            workspaces: [
                Workspace(name: "release", branch: "v0.42", chats: [
                    Chat(title: "Windows installer", agent: .claude, status: .done,
                         detail: "Done in 18m · $3.40"),
                ]),
            ]),
    ]

    static let questions: [Question] = [
        Question(computer: "MacBook Pro", workspace: "herdr-gpui", chat: "Fix mise discovery on SSH",
                 agent: .codex, prompt: "Run `git push origin fix/mise`?",
                 options: ["Allow once", "Always allow", "Deny"], isPermission: true),
        Question(computer: "studio", workspace: "site", chat: "Hero animation",
                 agent: .claude, prompt: "Which layout should the hero use?",
                 options: ["Split image left", "Full-bleed video", "Centered type"], isPermission: false),
    ]

    static var allChats: [(computer: Computer, workspace: Workspace, chat: Chat)] {
        computers.flatMap { computer in
            computer.workspaces.flatMap { workspace in
                workspace.chats.map { (computer, workspace, $0) }
            }
        }
    }
}
