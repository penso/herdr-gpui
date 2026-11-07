import Foundation

/// What typing into a terminal sends, in the key syntax of Herdr's
/// `pane.send_keys` (`POST /api/term/{pane}/keys`): modifiers joined by
/// `+`, then a named key, `f1` to `f12`, or one printable character.
public enum TermInput {
    /// The most keys one request carries, as the daemon allows.
    public static let maxKeys = 64

    /// Held modifiers for the next key.
    public struct Modifiers: OptionSet, Hashable, Sendable {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }
        public static let ctrl = Modifiers(rawValue: 1)
        public static let alt = Modifiers(rawValue: 2)
        public static let shift = Modifiers(rawValue: 4)

        /// What stays held after a key: ctrl and alt are for one key, and
        /// shift latches until it is tapped off, for runs of shift+arrows.
        public var afterKey: Modifiers { intersection(.shift) }
    }

    /// The keys a held shift changes. Others ignore it, so ^C stays an
    /// interrupt while shift is latched.
    private static let shiftable: Set<String> = ["down", "left", "right", "tab", "up"]

    /// One request's worth of input.
    public enum Send: Hashable, Sendable {
        case keys([String])
        /// Text too long for keys, sent whole; Herdr wraps it in a
        /// bracketed paste when the program asked for one.
        case paste(String)
    }

    /// Typed text as keys; `modifiers` apply to the first character only,
    /// as a sticky ctrl on the key row does. Text longer than one request
    /// (dictation, an autocorrected word) goes as a paste unless a
    /// modifier is held.
    public static func sends(for text: String, modifiers: Modifiers = []) -> [Send] {
        // A space with no key name (Japanese input's full-width U+3000)
        // would be dropped, and the daemon refuses whitespace as a key. A
        // paste would not press Enter, so a typed line stays keys.
        if modifiers.isEmpty, !text.contains(where: \.isNewline), text.contains(where: { $0.isWhitespace && key(for: $0) == nil }) {
            return [.paste(text)]
        }
        let keys = text.enumerated().compactMap { index, character in
            key(for: character, modifiers: index == 0 ? modifiers : [])
        }
        guard !keys.isEmpty else { return [] }
        if keys.count > maxKeys, modifiers.isEmpty, !text.contains("\n") { return [.paste(text)] }
        return stride(from: 0, to: keys.count, by: maxKeys).map {
            .keys(Array(keys[$0..<min($0 + maxKeys, keys.count)]))
        }
    }

    /// One character as a key, or `nil` for control characters it has no
    /// name for.
    public static func key(for character: Character, modifiers: Modifiers = []) -> String? {
        let name: String
        switch character {
        case " ": name = "space"
        case "\n", "\r", "\r\n": name = "enter"
        case "\t": name = "tab"
        case "+": name = "plus"
        case "-": name = "minus"
        case "/": name = "slash"
        case "`": name = "backtick"
        default:
            guard !character.isWhitespace, character.unicodeScalars.allSatisfy({ !$0.properties.isDefaultIgnorableCodePoint && $0.value >= 0x20 && $0.value != 0x7f })
            else { return nil }
            // A held ctrl names the letter, as terminals read `ctrl+c`.
            name = modifiers.contains(.ctrl) ? String(character).lowercased() : String(character)
        }
        return named(name, modifiers: modifiers)
    }

    /// A named key (`esc`, `up`) or a combination (`ctrl+c`) from the key
    /// row, with the `held` modifiers added.
    public static func pressed(_ name: String, held: Modifiers = []) -> String {
        let parts = name.split(separator: "+").map(String.init)
        let key = parts.last ?? name
        var modifiers = held
        if !shiftable.contains(key) { modifiers.remove(.shift) }
        for part in parts.dropLast() {
            switch part {
            case "ctrl": modifiers.insert(.ctrl)
            case "alt": modifiers.insert(.alt)
            case "shift": modifiers.insert(.shift)
            default: break
            }
        }
        return named(key, modifiers: modifiers)
    }

    /// `name` with `modifiers`, in the order the daemon checks them.
    public static func named(_ name: String, modifiers: Modifiers = []) -> String {
        var parts: [String] = []
        if modifiers.contains(.ctrl) { parts.append("ctrl") }
        if modifiers.contains(.alt) { parts.append("alt") }
        if modifiers.contains(.shift) { parts.append("shift") }
        parts.append(name)
        return parts.joined(separator: "+")
    }
}
