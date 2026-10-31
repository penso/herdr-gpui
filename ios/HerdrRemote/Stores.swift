import Foundation
import HerdrRemoteCore
import Observation

/// Which computer and pane a view is about.
struct PaneKey: Codable, Hashable, Sendable {
    let device: String
    let pane: String
}

/// One open chat: its transcript as the reducer keeps it, starting from
/// the cache and following the live socket.
@MainActor
@Observable
final class ChatStore {
    let key: PaneKey
    private(set) var transcript: Transcript?
    private(set) var cursor: Cursor?
    private(set) var connected = false
    /// The transcript came from the cache and nothing newer arrived yet.
    private(set) var stale = true
    /// Set when a change did not fit; the fleet fetches a snapshot.
    private(set) var diverged = false
    /// Prompts this phone sent that the transcript does not show yet.
    private(set) var outgoing: [Outgoing] = []

    init(key: PaneKey, cached: Cache.CachedChat?) {
        self.key = key
        transcript = cached?.transcript
        cursor = cached?.cursor
    }

    func apply(_ event: LiveEvent) {
        switch event {
        case let .chatSnapshot(snapshot):
            transcript = snapshot.transcript
            cursor = snapshot.cursor
            connected = snapshot.connected
            stale = false
            diverged = false
        case let .chatResumed(_, cursor, connected):
            self.cursor = cursor
            self.connected = connected
            stale = false
        case let .chatChanges(_, changes):
            // After a change that did not fit, wait for the snapshot.
            guard !diverged, var current = transcript else { return }
            for item in changes {
                do {
                    try current.apply(item.change)
                } catch {
                    diverged = true
                    return
                }
                cursor = item.cursor
            }
            transcript = current
        case let .chatConnection(_, connected):
            self.connected = connected
        default:
            break
        }
        settle()
    }

    /// Shows a prompt at once, before the daemon has it, with the uploads
    /// of the pictures it carries.
    func sending(_ text: String, uploads: [UploadProgress]) -> Outgoing.ID {
        let prompt = Outgoing(
            text: text.trimmingCharacters(in: .whitespacesAndNewlines),
            uploads: uploads,
            after: transcript?.entries.last?.id
        )
        outgoing.append(prompt)
        return prompt.id
    }

    /// Its pictures are on the computer; the prompt itself is on its way.
    func uploaded(_ id: Outgoing.ID) {
        guard let index = outgoing.firstIndex(where: { $0.id == id }) else { return }
        outgoing[index].stage = .toComputer
    }

    /// A refused prompt goes back to the composer, so it leaves here. An
    /// accepted one waits for its echo, but not forever: a turn whose
    /// text the agent rewrote would never match.
    func sent(_ id: Outgoing.ID, failed: Bool) {
        guard !failed else {
            outgoing.removeAll { $0.id == id }
            return
        }
        if let index = outgoing.firstIndex(where: { $0.id == id }) { outgoing[index].stage = .toAgent }
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(30))
            self?.outgoing.removeAll { $0.id == id }
        }
    }

    /// Drops the prompts the transcript now shows, as a turn or queued.
    private func settle() {
        guard !outgoing.isEmpty, let transcript else { return }
        outgoing.removeAll { $0.echoed(in: transcript) }
    }
}

/// How much of one picture's upload has been sent.
@MainActor
@Observable
final class UploadProgress {
    var fraction = 0.0
}

/// A prompt sent from this phone, shown until the transcript echoes it.
struct Outgoing: Identifiable, Equatable {
    enum Stage: Equatable {
        /// Its pictures are uploading.
        case uploading
        /// The prompt is on its way to the computer.
        case toComputer
        /// The computer took it; the agent's chat has not shown it yet.
        case toAgent
    }

    let id = UUID()
    let text: String
    let uploads: [UploadProgress]
    /// The transcript's last entry when it was sent; the echo follows it.
    let after: EntryId?
    var stage: Stage

    init(text: String, uploads: [UploadProgress], after: EntryId?) {
        self.text = text
        self.uploads = uploads
        self.after = after
        stage = uploads.isEmpty ? .toComputer : .uploading
    }

    static func == (lhs: Outgoing, rhs: Outgoing) -> Bool {
        lhs.id == rhs.id && lhs.stage == rhs.stage
    }

    /// How far along the whole send is: uploads fill most of the bar, the
    /// computer and the agent each the rest.
    @MainActor
    var progress: Double {
        switch stage {
        case .uploading:
            let sent = uploads.map(\.fraction).reduce(0, +) / Double(max(uploads.count, 1))
            return 0.8 * sent
        case .toComputer: return uploads.isEmpty ? 0.3 : 0.85
        case .toAgent: return 0.95
        }
    }

    func echoed(in transcript: Transcript) -> Bool {
        if transcript.queued.contains(where: { $0.text.trimmingCharacters(in: .whitespacesAndNewlines) == text }) {
            return true
        }
        let entries = transcript.entries
        // A trimmed transcript may have dropped `after`; then look at all.
        let start = after.flatMap { id in entries.firstIndex { $0.id == id }.map { $0 + 1 } } ?? 0
        return entries[start...].contains { entry in
            // Attached files follow the text as extra lines.
            guard case let .user(said, _) = entry.kind else { return false }
            return said.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix(text)
        }
    }
}

/// One open terminal pane's screen.
@MainActor
@Observable
final class TermStore {
    let key: PaneKey
    private(set) var screen: TermScreen?
    /// The size the terminal was asked to take on this screen.
    @ObservationIgnored var size: TermSize?

    init(key: PaneKey) { self.key = key }

    func apply(_ screen: TermScreen) { self.screen = screen }
}

/// The composer's text shortcuts, kept on this phone.
@MainActor
@Observable
final class ShortcutStore {
    private static let key = "textShortcuts"
    private(set) var shortcuts: [TextShortcut]

    init() {
        shortcuts = TextShortcut.load(UserDefaults.standard.data(forKey: Self.key))
    }

    /// Adds the shortcut, or replaces the one with its id.
    func save(_ shortcut: TextShortcut) {
        if let index = shortcuts.firstIndex(where: { $0.id == shortcut.id }) {
            shortcuts[index] = shortcut
        } else {
            shortcuts.append(shortcut)
        }
        persist()
    }

    func delete(at offsets: IndexSet) {
        shortcuts.remove(atOffsets: offsets)
        persist()
    }

    func move(from offsets: IndexSet, to destination: Int) {
        shortcuts.move(fromOffsets: offsets, toOffset: destination)
        persist()
    }

    private func persist() {
        guard let data = try? TextShortcut.save(shortcuts) else { return }
        UserDefaults.standard.set(data, forKey: Self.key)
    }
}
