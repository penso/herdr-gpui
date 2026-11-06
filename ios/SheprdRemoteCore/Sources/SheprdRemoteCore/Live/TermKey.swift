import Foundation

/// A button on a terminal's key rows: the keys it presses, in order, and
/// what it shows.
public struct TermKey: Codable, Hashable, Identifiable, Sendable {
    public var id: UUID
    /// What the button shows; blank shows its keys' symbols.
    public var title: String
    /// Herdr key names (`TermInput`), sent together.
    public var keys: [String]

    public init(id: UUID = UUID(), title: String = "", keys: [String]) {
        self.id = id
        self.title = title
        self.keys = keys
    }

    public init(id: UUID = UUID(), special: Special) {
        self.init(id: id, keys: [special.rawValue])
    }

    /// A key the app handles itself rather than sending to the terminal.
    public enum Special: String, CaseIterable, Sendable {
        case dictation = "@dictation"
        /// Shows and hides the keyboard. It cannot be deleted: the keyboard
        /// could not come back.
        case keyboard = "@keyboard"

        public var symbol: String {
            switch self {
            case .dictation: "🎙\u{FE0E}"
            case .keyboard: "⌨\u{FE0E}"
            }
        }

        public var name: String {
            switch self {
            case .dictation: "Dictation"
            case .keyboard: "Keyboard"
            }
        }
    }

    public var special: Special? {
        keys.count == 1 ? Special(rawValue: keys[0]) : nil
    }

    /// One key the key row editor offers.
    public struct Choice: Hashable, Sendable {
        public let key: String
        public let symbol: String
        /// What the key shows instead of its symbol when keys show words.
        public let word: String?
        /// What VoiceOver says, and the editor lists.
        public let name: String

        init(key: String, symbol: String, word: String? = nil, name: String) {
            self.key = key
            self.symbol = symbol
            self.word = word
            self.name = name
        }
    }

    public static let choices: [Choice] = [
        Choice(key: "esc", symbol: "⎋", word: "esc", name: "Escape"),
        Choice(key: "tab", symbol: "⇥", name: "Tab"),
        Choice(key: "shift+tab", symbol: "⇤", name: "Shift Tab"),
        Choice(key: "enter", symbol: "⏎", name: "Enter"),
        // A new line without Enter: Claude Code and Codex read Ctrl-J as
        // one, and write it Shift-Enter.
        Choice(key: "ctrl+j", symbol: "⇧⏎", name: "New line"),
        Choice(key: "backspace", symbol: "⌫", name: "Delete"),
        Choice(key: "up", symbol: "↑", name: "Up"),
        Choice(key: "down", symbol: "↓", name: "Down"),
        Choice(key: "left", symbol: "←", name: "Left"),
        Choice(key: "right", symbol: "→", name: "Right"),
        Choice(key: "|", symbol: "|", name: "Pipe"),
        Choice(key: "slash", symbol: "/", name: "Slash"),
        Choice(key: "minus", symbol: "-", name: "Minus"),
        Choice(key: "~", symbol: "~", name: "Tilde"),
        Choice(key: "backtick", symbol: "`", name: "Backtick"),
    ] + "acdeklruwz".map { letter in
        Choice(key: "ctrl+\(letter)", symbol: "^\(letter.uppercased())", name: "Control \(letter.uppercased())")
    }

    public static func choice(_ key: String) -> Choice? {
        choices.first { $0.key == key }
    }

    /// The title, or the keys' words (esc) or, with `symbols`, symbols (⎋).
    public func label(symbols: Bool = false) -> String {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.isEmpty else { return trimmed }
        return keys.map { key in
            guard let choice = Self.choice(key) else { return Special(rawValue: key)?.symbol ?? key }
            return symbols ? choice.symbol : choice.word ?? choice.symbol
        }.joined()
    }

    /// A fixed modifier key's label: its name, or with `symbols` ⌃ ⌥ ⇧.
    public static func modifier(_ name: String, symbols: Bool) -> String {
        guard symbols else { return name }
        return ["ctrl": "⌃", "alt": "⌥", "shift": "⇧"][name] ?? name
    }

    /// The title, or the keys' names.
    public var spoken: String {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? keys.map { Self.choice($0)?.name ?? Special(rawValue: $0)?.name ?? $0 }.joined(separator: ", ") : trimmed
    }

    /// Held down, one key repeats as a keyboard's does. Enter and a run of
    /// keys do not, so a long press cannot answer an agent twice.
    public var repeats: Bool { keys.count == 1 && keys != ["enter"] && special == nil }

    /// One of the keys the editor always offers, as it offers it, rather
    /// than one the person made with a label or several presses.
    public var isStock: Bool {
        guard title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, keys.count == 1 else { return false }
        return Self.choice(keys[0]) != nil || Special(rawValue: keys[0]) != nil
    }
}

/// The keys of a terminal's two key rows, after the modifiers that start
/// the top row, and the spare keys kept off them, ready to add back.
public struct TermKeyLayout: Codable, Hashable, Sendable {
    public var top: [TermKey]
    public var bottom: [TermKey]
    /// Shown as More Keys: every key in neither row, so none is ever lost.
    public var spare: [TermKey]

    public init(top: [TermKey], bottom: [TermKey], spare: [TermKey] = []) {
        self.top = top
        self.bottom = bottom
        self.spare = spare
    }

    /// Where a key is: a row, or spare.
    public enum Row: String, CaseIterable, Sendable {
        case top, bottom, spare
    }

    public subscript(row: Row) -> [TermKey] {
        get {
            switch row {
            case .top: top
            case .bottom: bottom
            case .spare: spare
            }
        }
        set {
            switch row {
            case .top: top = newValue
            case .bottom: bottom = newValue
            case .spare: spare = newValue
            }
        }
    }

    public func row(of key: TermKey) -> Row? {
        Row.allCases.first { self[$0].contains { $0.id == key.id } }
    }

    /// Replaces `key` where it is in `row`, or adds it at the row's end,
    /// taking it out of wherever else it was. A stock key edited into
    /// another is offered again in More Keys.
    public mutating func put(_ key: TermKey, in row: Row) {
        if let index = self[row].firstIndex(where: { $0.id == key.id }) {
            self[row][index] = key
        } else {
            for other in Row.allCases { self[other].removeAll { $0.id == key.id } }
            self[row].append(key)
        }
        complete()
    }

    /// A line of the editor's one list of both rows and the spare keys.
    public enum Entry: Hashable, Identifiable, Sendable {
        case key(TermKey)
        /// The bottom row's keys follow it.
        case bottomRow
        /// The spare keys follow it.
        case spareKeys

        public var id: String {
            switch self {
            case let .key(key): key.id.uuidString
            case .bottomRow: "bottomRow"
            case .spareKeys: "spareKeys"
            }
        }

        public var termKey: TermKey? {
            if case let .key(key) = self { key } else { nil }
        }
    }

    /// The top row, a marker, the bottom row, a marker, then the spare
    /// keys, so a key can be dragged anywhere among them.
    public var entries: [Entry] {
        top.map(Entry.key) + [.bottomRow] + bottom.map(Entry.key) + [.spareKeys] + spare.map(Entry.key)
    }

    /// Moves lines of `entries` as a list does: the keys between the markers
    /// make the bottom row, and those after the second are spare. A move
    /// that would make the keyboard key spare is refused.
    public mutating func move(fromOffsets source: IndexSet, toOffset destination: Int) {
        let list = entries
        let moved = source.map { list[$0] }
        var rest = list.indices.filter { !source.contains($0) }.map { list[$0] }
        rest.insert(contentsOf: moved, at: destination - source.count { $0 < destination })
        guard let bottomRow = rest.firstIndex(of: .bottomRow), let spareKeys = rest.firstIndex(of: .spareKeys), bottomRow < spareKeys
        else { return }
        let spare = rest[spareKeys...].compactMap(\.termKey)
        guard !spare.contains(where: { $0.special == .keyboard }) else { return }
        top = rest[..<bottomRow].compactMap(\.termKey)
        bottom = rest[bottomRow..<spareKeys].compactMap(\.termKey)
        self.spare = spare
    }

    /// Whether deleting `key` does anything: the keyboard key stays, and so
    /// does the last copy of a stock key in More Keys, which always offers it.
    public func canDelete(_ key: TermKey) -> Bool {
        guard key.special != .keyboard else { return false }
        guard row(of: key) == .spare, key.isStock else { return true }
        return (top + bottom + spare).contains { $0.id != key.id && $0.isStock && $0.keys == key.keys }
    }

    /// Deletes the keys at lines of `entries`: one in a row becomes the
    /// first spare key, and a spare key goes when `canDelete` allows it.
    public mutating func delete(atOffsets offsets: IndexSet) {
        let list = entries
        let picked = offsets.compactMap { list[$0].termKey }.filter { $0.special != .keyboard }
        let spareIDs = Set(spare.map(\.id))
        let fromRows = picked.filter { !spareIDs.contains($0.id) }
        let moving = Set(fromRows.map(\.id))
        top.removeAll { moving.contains($0.id) }
        bottom.removeAll { moving.contains($0.id) }
        spare = fromRows + spare
        // One at a time, so deleting every copy at once still leaves one.
        for key in picked where spareIDs.contains(key.id) && canDelete(key) {
            spare.removeAll { $0.id == key.id }
        }
    }

    /// Back to the default rows, with the keys the person made kept spare.
    public mutating func restoreDefaults() {
        let made = (top + bottom + spare).filter { !$0.isStock }
        self = Self.defaults
        // New ids: a made key may be a renamed default that kept its id.
        for row in Row.allCases { self[row] = self[row].map { TermKey(title: $0.title, keys: $0.keys) } }
        spare = made + spare
    }

    /// The rows hold the default keys, in order. Ids differ by launch, so
    /// they are not compared.
    public var isDefault: Bool {
        func faces(_ keys: [TermKey]) -> [[String]] { keys.map { [$0.title] + $0.keys } }
        return faces(top) == faces(Self.defaults.top) && faces(bottom) == faces(Self.defaults.bottom)
    }

    /// Adds every stock key in neither row nor the spare keys to the spare
    /// keys, so More Keys always offers all of them.
    private mutating func complete() {
        let placed = Set((top + bottom + spare).filter(\.isStock).flatMap(\.keys))
        let stock = TermKey.choices.map(\.key) + TermKey.Special.allCases.map(\.rawValue)
        for key in stock where !placed.contains(key) && key != TermKey.Special.keyboard.rawValue {
            spare.append(TermKey(keys: [key]))
        }
    }

    public static let defaults: TermKeyLayout = {
        var layout = TermKeyLayout(
            top: ["esc", "tab", "enter", "ctrl+j", "|", "slash", "minus", "~", "backtick", "up"].map { TermKey(keys: [$0]) },
            bottom: [
                "ctrl+c", TermKey.Special.dictation.rawValue, "ctrl+d", "ctrl+z", "ctrl+r", "ctrl+l", "ctrl+a", "ctrl+e",
                "left", "down", "right", TermKey.Special.keyboard.rawValue,
            ].map { TermKey(keys: [$0]) }
        )
        layout.complete()
        return layout
    }()

    /// The saved rows, or the defaults when nothing was saved yet or it
    /// cannot be read.
    public static func load(_ data: Data?) -> TermKeyLayout {
        guard let data, var saved = try? JSONDecoder().decode(TermKeyLayout.self, from: data) else { return defaults }
        // A later build may offer keys that were not offered when this was saved.
        saved.complete()
        return saved
    }

    public static func save(_ layout: TermKeyLayout) throws -> Data {
        try JSONEncoder().encode(layout)
    }
}
