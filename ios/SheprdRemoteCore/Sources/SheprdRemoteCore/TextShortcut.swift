import Foundation

/// A saved prompt the composer's `+` menu offers, such as asking the agent
/// to carry on.
public struct TextShortcut: Codable, Hashable, Identifiable, Sendable {
    public var id: UUID
    /// What the menu shows.
    public var title: String
    /// What the chat gets.
    public var text: String

    public init(id: UUID = UUID(), title: String, text: String) {
        self.id = id
        self.title = title
        self.text = text
    }

    /// The title, or the start of the text when the title is blank.
    public var label: String {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty { return trimmed }
        let line = text.trimmingCharacters(in: .whitespacesAndNewlines).prefix { !$0.isNewline }
        return line.count > 40 ? String(line.prefix(40)) + "…" : String(line)
    }

    /// Worth saving: there is text to send.
    public var isUsable: Bool { !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    /// The shortcut a new install starts with.
    public static let proceed = TextShortcut(
        id: UUID(uuidString: "6F1C2B0E-3A4D-4E5F-8A9B-0C1D2E3F4A5B") ?? UUID(),
        title: "Proceed",
        text: "Proceed with anything that is left to do."
    )

    public static let defaults = [proceed]

    /// The saved list, or the defaults when nothing was saved yet. A list
    /// that cannot be read is kept as the defaults rather than emptied.
    public static func load(_ data: Data?) -> [TextShortcut] {
        guard let data, let saved = try? JSONDecoder().decode([TextShortcut].self, from: data) else { return defaults }
        return saved
    }

    public static func save(_ shortcuts: [TextShortcut]) throws -> Data {
        try JSONEncoder().encode(shortcuts)
    }
}
