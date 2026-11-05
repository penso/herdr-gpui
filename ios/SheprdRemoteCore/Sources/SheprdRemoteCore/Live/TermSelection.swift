import Foundation

/// Cells selected on a terminal screen, as terminals select: from where a
/// long press began (`anchor`) to where the finger is (`head`), in reading
/// order, by cell, so a wide character is selected by either of its cells.
public struct TermSelection: Equatable, Sendable {
    public struct Position: Comparable, Hashable, Sendable {
        public var row: Int
        public var column: Int

        public init(row: Int, column: Int) {
            self.row = row
            self.column = column
        }

        public static func < (lhs: Position, rhs: Position) -> Bool {
            (lhs.row, lhs.column) < (rhs.row, rhs.column)
        }
    }

    public var anchor: Position
    public var head: Position

    public init(anchor: Position, head: Position) {
        self.anchor = anchor
        self.head = head
    }

    public var start: Position { min(anchor, head) }
    public var end: Position { max(anchor, head) }

    /// The columns selected in `row`, whose last cell is `last`.
    public func columns(in row: Int, last: Int) -> ClosedRange<Int>? {
        guard (start.row...end.row).contains(row) else { return nil }
        let from = row == start.row ? start.column : 0
        let to = row == end.row ? end.column : last
        return from <= to ? from...to : nil
    }
}

extension TermScreen {
    /// A row's characters, each with the column it starts at and the cells
    /// it covers; hidden text (SGR 8) is blank.
    static func laidOut(_ row: [Run]) -> [(character: Character, column: Int, cells: Int)] {
        var column = 0
        var laid: [(character: Character, column: Int, cells: Int)] = []
        for run in row {
            for character in run.text {
                let cells = cells(String(character))
                laid.append((run.hidden ? " " : character, column, cells))
                column += cells
            }
        }
        return laid
    }

    /// The word under a long press: the non-blank characters touching its
    /// cell, or the cell alone.
    public func word(at position: TermSelection.Position) -> TermSelection {
        let alone = TermSelection(anchor: position, head: position)
        guard rows.indices.contains(position.row) else { return alone }
        let laid = Self.laidOut(rows[position.row])
        guard let index = laid.firstIndex(where: { position.column < $0.column + $0.cells }), laid[index].column <= position.column,
              !laid[index].character.isWhitespace
        else { return alone }
        var first = index
        while first > 0, !laid[first - 1].character.isWhitespace { first -= 1 }
        var last = index
        while last + 1 < laid.count, !laid[last + 1].character.isWhitespace { last += 1 }
        return TermSelection(anchor: .init(row: position.row, column: laid[first].column),
                             head: .init(row: position.row, column: laid[last].column + laid[last].cells - 1))
    }

    /// Every cell on the screen.
    public var everything: TermSelection {
        let widest = rows.map { Self.laidOut($0).reduce(0) { $0 + $1.cells } }.max() ?? 0
        return TermSelection(anchor: .init(row: 0, column: 0), head: .init(row: max(rows.count - 1, 0), column: max(widest - 1, 0)))
    }

    /// The selected text: a line per row, each without the blanks that
    /// end it, and a character partly selected included whole.
    public func text(_ selection: TermSelection) -> String {
        rows.indices.compactMap { index -> String? in
            let laid = Self.laidOut(rows[index])
            guard let columns = selection.columns(in: index, last: Int.max - 1) else { return nil }
            let line = String(laid.filter { $0.column <= columns.upperBound && columns.lowerBound < $0.column + $0.cells }.map(\.character))
            return String(line.reversed().drop(while: \.isWhitespace).reversed())
        }
        .joined(separator: "\n")
    }
}
