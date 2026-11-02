import Foundation

/// An agent's answer split into the blocks the chat draws natively: the
/// common Markdown that agents write (fences, headings, lists, quotes,
/// tables, rules), with each block's inline text left for
/// `AttributedString(markdown:)`. It is line based, so text appended while
/// an answer streams only ever changes the last block or adds new ones,
/// and an unclosed fence reads as a code block so far.
public enum MarkdownBlock: Hashable, Sendable {
    case paragraph(String)
    case heading(level: Int, text: String)
    case code(language: String?, text: String)
    case list(ordered: Bool, start: Int, items: [MarkdownListItem])
    case quote(String)
    case table(header: [String], rows: [[String]])
    case rule

    public static func parse(_ text: String) -> [MarkdownBlock] {
        var parser = Parser(lines: text.split(separator: "\n", omittingEmptySubsequences: false).map {
            $0.hasSuffix("\r") ? String($0.dropLast()) : String($0)
        })
        return parser.blocks()
    }
}

public struct MarkdownListItem: Hashable, Sendable {
    public var text: String
    /// Nesting, from 0.
    public var depth: Int
    /// A task item's box: `true` when ticked, `nil` for a plain item.
    public var checked: Bool?

    public init(text: String, depth: Int = 0, checked: Bool? = nil) {
        self.text = text
        self.depth = depth
        self.checked = checked
    }
}

private struct Parser {
    let lines: [String]
    var index = 0

    mutating func blocks() -> [MarkdownBlock] {
        var blocks: [MarkdownBlock] = []
        while index < lines.count {
            let line = lines[index]
            if line.trimmingCharacters(in: .whitespaces).isEmpty {
                index += 1
            } else if let fence = Self.fence(line) {
                blocks.append(code(fence))
            } else if let heading = Self.heading(line) {
                blocks.append(heading)
                index += 1
            } else if Self.isRule(line) {
                blocks.append(.rule)
                index += 1
            } else if Self.quoted(line) != nil {
                blocks.append(quote())
            } else if Self.listItem(line) != nil {
                blocks.append(list())
            } else if index + 1 < lines.count, line.contains("|"), Self.isDelimiterRow(lines[index + 1]) {
                blocks.append(table())
            } else {
                blocks.append(paragraph())
            }
        }
        return blocks
    }

    // MARK: Blocks

    private mutating func code(_ fence: (marker: Character, length: Int, language: String?)) -> MarkdownBlock {
        index += 1
        var body: [String] = []
        while index < lines.count {
            let line = lines[index]
            index += 1
            let trimmed = line.drop { $0 == " " }
            let run = trimmed.prefix { $0 == fence.marker }.count
            if run >= fence.length, trimmed.dropFirst(run).allSatisfy(\.isWhitespace) { break }
            body.append(line)
        }
        return .code(language: fence.language, text: body.joined(separator: "\n"))
    }

    private mutating func quote() -> MarkdownBlock {
        var body: [String] = []
        while index < lines.count, let inner = Self.quoted(lines[index]) {
            body.append(inner)
            index += 1
        }
        return .quote(body.joined(separator: "\n"))
    }

    private mutating func list() -> MarkdownBlock {
        guard let first = Self.listItem(lines[index]) else { return paragraph() }
        let base = first.indent
        var items: [MarkdownListItem] = []
        while index < lines.count {
            let line = lines[index]
            if let item = Self.listItem(line), item.ordered == first.ordered || item.indent > base {
                items.append(MarkdownListItem(text: item.text, depth: max(0, (item.indent - base) / 2), checked: item.checked))
                index += 1
            } else if line.trimmingCharacters(in: .whitespaces).isEmpty {
                // A blank line ends the list unless another item follows.
                let next = lines[(index + 1)...].first { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
                guard let next, let item = Self.listItem(next), item.ordered == first.ordered || item.indent > base
                else { break }
                index += 1
            } else if line.first == " " || line.first == "\t", !items.isEmpty, Self.fence(line) == nil {
                items[items.count - 1].text += "\n" + line.trimmingCharacters(in: .whitespaces)
                index += 1
            } else {
                break
            }
        }
        return .list(ordered: first.ordered, start: first.number, items: items)
    }

    private mutating func table() -> MarkdownBlock {
        let header = Self.cells(lines[index])
        index += 2
        var rows: [[String]] = []
        while index < lines.count, lines[index].contains("|"), !lines[index].trimmingCharacters(in: .whitespaces).isEmpty {
            var row = Self.cells(lines[index])
            if row.count < header.count { row += Array(repeating: "", count: header.count - row.count) }
            rows.append(Array(row.prefix(header.count)))
            index += 1
        }
        return .table(header: header, rows: rows)
    }

    private mutating func paragraph() -> MarkdownBlock {
        var body: [String] = []
        while index < lines.count {
            let line = lines[index]
            if line.trimmingCharacters(in: .whitespaces).isEmpty { break }
            if !body.isEmpty, Self.startsBlock(line) { break }
            body.append(line)
            index += 1
        }
        return .paragraph(body.joined(separator: "\n"))
    }

    // MARK: Lines

    private static func startsBlock(_ line: String) -> Bool {
        fence(line) != nil || heading(line) != nil || isRule(line) || quoted(line) != nil || listItem(line) != nil
    }

    /// An opening fence: three or more backticks or tildes, indented at
    /// most three spaces, and the language after it.
    static func fence(_ line: String) -> (marker: Character, length: Int, language: String?)? {
        let indent = line.prefix { $0 == " " }.count
        guard indent <= 3 else { return nil }
        let rest = line.dropFirst(indent)
        guard let marker = rest.first, marker == "`" || marker == "~" else { return nil }
        let length = rest.prefix { $0 == marker }.count
        guard length >= 3 else { return nil }
        let info = rest.dropFirst(length).trimmingCharacters(in: .whitespaces)
        if marker == "`", info.contains("`") { return nil }
        let language = info.split(separator: " ").first.map(String.init)
        return (marker, length, language)
    }

    static func heading(_ line: String) -> MarkdownBlock? {
        let indent = line.prefix { $0 == " " }.count
        guard indent <= 3 else { return nil }
        let rest = line.dropFirst(indent)
        let level = rest.prefix { $0 == "#" }.count
        guard (1...6).contains(level) else { return nil }
        let after = rest.dropFirst(level)
        guard after.isEmpty || after.first == " " || after.first == "\t" else { return nil }
        var text = after.trimmingCharacters(in: .whitespaces)
        // A closing run of #s is decoration.
        if let closing = text.lastIndex(where: { $0 != "#" }), text[closing] == " " {
            text = String(text[..<closing]).trimmingCharacters(in: .whitespaces)
        } else if text.allSatisfy({ $0 == "#" }) {
            text = ""
        }
        return .heading(level: level, text: text)
    }

    static func isRule(_ line: String) -> Bool {
        let marks = line.filter { !$0.isWhitespace }
        guard let mark = marks.first, "-*_".contains(mark), marks.count >= 3 else { return false }
        return marks.allSatisfy { $0 == mark } && line.prefix { $0 == " " }.count <= 3
    }

    static func quoted(_ line: String) -> String? {
        let rest = line.drop { $0 == " " }
        guard rest.first == ">", line.count - rest.count <= 3 else { return nil }
        let inner = rest.dropFirst()
        return String(inner.first == " " ? inner.dropFirst() : inner)
    }

    struct Item {
        var indent: Int
        var ordered: Bool
        var number: Int
        var text: String
        var checked: Bool?
    }

    static func listItem(_ line: String) -> Item? {
        let indent = line.prefix { $0 == " " || $0 == "\t" }.reduce(0) { $0 + ($1 == "\t" ? 4 : 1) }
        let rest = line.drop { $0 == " " || $0 == "\t" }
        var ordered = false
        var number = 1
        var body: Substring
        if let mark = rest.first, "-*+".contains(mark) {
            body = rest.dropFirst()
        } else {
            let digits = rest.prefix(while: \.isNumber)
            guard (1...9).contains(digits.count), let value = Int(digits) else { return nil }
            let after = rest.dropFirst(digits.count)
            guard let mark = after.first, mark == "." || mark == ")" else { return nil }
            ordered = true
            number = value
            body = after.dropFirst()
        }
        guard body.isEmpty || body.first == " " || body.first == "\t" else { return nil }
        var text = body.trimmingCharacters(in: .whitespaces)
        var checked: Bool?
        if text.hasPrefix("[ ] ") || text == "[ ]" {
            checked = false
        } else if text.lowercased().hasPrefix("[x] ") || text.lowercased() == "[x]" {
            checked = true
        }
        if checked != nil { text = String(text.dropFirst(3)).trimmingCharacters(in: .whitespaces) }
        return Item(indent: indent, ordered: ordered, number: number, text: text, checked: checked)
    }

    /// `| :--- | ---: |`: what makes the line above a table's header.
    static func isDelimiterRow(_ line: String) -> Bool {
        guard line.contains("-") else { return false }
        let cells = cells(line)
        return !cells.isEmpty && cells.allSatisfy { cell in
            var body = Substring(cell)
            if body.first == ":" { body = body.dropFirst() }
            if body.last == ":" { body = body.dropLast() }
            return !body.isEmpty && body.allSatisfy { $0 == "-" }
        }
    }

    /// A row's cells, split on pipes outside code spans and escapes, with
    /// the outer pipes dropped.
    static func cells(_ line: String) -> [String] {
        var cells: [String] = []
        var current = ""
        var inCode = false
        var escaped = false
        for character in line.trimmingCharacters(in: .whitespaces) {
            if escaped {
                current.append(character)
                escaped = false
            } else if character == "\\" {
                escaped = true
                current.append(character)
            } else if character == "`" {
                inCode.toggle()
                current.append(character)
            } else if character == "|", !inCode {
                cells.append(current)
                current = ""
            } else {
                current.append(character)
            }
        }
        cells.append(current)
        if cells.first?.trimmingCharacters(in: .whitespaces).isEmpty == true { cells.removeFirst() }
        if cells.last?.trimmingCharacters(in: .whitespaces).isEmpty == true { cells.removeLast() }
        return cells.map {
            $0.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "\\|", with: "|")
        }
    }
}
