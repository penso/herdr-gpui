extension TermScreen {
    /// The background `row` leaves for the rest of its line: its last run's,
    /// as a program erasing to the end of a line paints it. Reverse video
    /// is not carried into erased cells, so a cursor a program draws as one
    /// reversed cell at the end of its input line paints only that cell.
    public static func fill(_ row: [Run]) -> String? {
        guard let last = row.last, !last.inverse else { return nil }
        return last.bg
    }
}
