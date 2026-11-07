import Foundation

/// What can be named out loud, as the fleet last showed it. Everything a
/// spoken command resolves to comes from here, so it can only ever name
/// something that exists.
public struct VoiceCatalog: Hashable, Sendable {
    public var computers: [Computer]
    public var chats: [Chat]
    /// The computer and agent last used, which fill in what was not said.
    public var recentDevice: String?
    public var recentAgent: String?

    public init(computers: [Computer], chats: [Chat], recentDevice: String? = nil, recentAgent: String? = nil) {
        self.computers = computers
        self.chats = chats
        self.recentDevice = recentDevice
        self.recentAgent = recentAgent
    }

    public struct Computer: Hashable, Sendable, Identifiable {
        /// `Computer.device`.
        public var id: String
        public var name: String
        /// Whether a workspace can be made there now.
        public var online: Bool
        public var home: Home?
        /// What it offers. Without it, only `home`'s checkouts are known.
        public var options: NewOptions?

        public init(id: String, name: String, online: Bool, home: Home? = nil, options: NewOptions? = nil) {
            self.id = id
            self.name = name
            self.online = online
            self.home = home
            self.options = options
        }

        /// Its projects, the most recently used first, one per name.
        public var projects: [Project] {
            var seen = Set<String>()
            let checkouts = (options?.folders ?? home?.workspaces.map(\.cwd) ?? [])
            return checkouts.compactMap { folder in
                let name = repository(of: folder) ?? URL(filePath: folder).lastPathComponent
                guard seen.insert(name).inserted else { return nil }
                return Project(name: name, folder: home?.checkout(ofRepository: name).flatMap { offered($0) ? $0 : nil } ?? folder)
            }
        }

        private func repository(of folder: String) -> String? {
            home?.workspaces.first { $0.cwd == folder }?.worktree?.label
        }

        private func offered(_ folder: String) -> Bool {
            options?.folders.contains(folder) ?? true
        }

        public var agents: [AgentChoice] { options?.agents ?? [] }
    }

    public struct Project: Hashable, Sendable, Identifiable {
        public var name: String
        public var folder: String
        public var id: String { folder }

        public init(name: String, folder: String) {
            self.name = name
            self.folder = folder
        }
    }

    public struct Chat: Hashable, Sendable, Identifiable {
        public var device: String
        public var pane: String
        public var title: String
        public var computer: String
        /// The question waiting, if any.
        public var question: Permission?
        public var id: String { "\(device)/\(pane)" }

        public init(device: String, pane: String, title: String, computer: String, question: Permission? = nil) {
            self.device = device
            self.pane = pane
            self.title = title
            self.computer = computer
            self.question = question
        }
    }

    /// Every agent any computer offers, once each, in the order first seen.
    public var agents: [AgentChoice] {
        var seen = Set<String>()
        return computers.flatMap(\.agents).filter { seen.insert($0.id).inserted }
    }

    /// Every project name, once each, most recent first.
    public var projectNames: [String] {
        var seen = Set<String>()
        return computers.flatMap(\.projects).map(\.name).filter { seen.insert($0).inserted }
    }

    /// Names to prime the transcriber with, so it can spell them.
    public var vocabulary: [String] {
        var seen = Set<String>()
        let names = ["worktree"] + computers.map(\.name) + projectNames + agents.map(\.name) + chats.map(\.title)
        return names.filter { seen.insert($0).inserted }
    }
}
