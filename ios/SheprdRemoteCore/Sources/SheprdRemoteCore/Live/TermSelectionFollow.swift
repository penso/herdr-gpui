import Foundation

extension TermScreen {
    /// `selection`, made on `old`, on this screen: where its rows' text is
    /// now, moved by the nearest whole rows when output scrolled it (up
    /// first, as output pushes rows up), or nil when that text changed or
    /// left the screen. The screen carries no scroll offset, so its rows'
    /// text is what places it.
    public func following(_ selection: TermSelection, from old: TermScreen) -> TermSelection? {
        let selected = selection.start.row...selection.end.row
        guard old.rows.indices.contains(selected.lowerBound), old.rows.indices.contains(selected.upperBound) else { return nil }
        let wanted = selected.map { Self.line(old.rows[$0]) }
        let moves = [0] + (1..<max(rows.count, 1)).flatMap { [-$0, $0] }
        guard let move = moves.first(where: { move in
            rows.indices.contains(selected.lowerBound + move) && rows.indices.contains(selected.upperBound + move)
                && zip(selected, wanted).allSatisfy { Self.line(rows[$0 + move]) == $1 }
        }) else { return nil }
        return selection.moved(rows: move)
    }

    private static func line(_ row: [Run]) -> String {
        row.map(\.text).joined()
    }
}

extension TermSelection {
    /// The same cells `rows` rows down, or up when negative.
    public func moved(rows: Int) -> TermSelection {
        TermSelection(anchor: .init(row: anchor.row + rows, column: anchor.column),
                      head: .init(row: head.row + rows, column: head.column))
    }
}
