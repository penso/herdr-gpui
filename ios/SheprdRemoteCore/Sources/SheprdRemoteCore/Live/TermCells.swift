import Foundation

extension TermScreen {
    /// The terminal cells `text` covers: two for each Chinese, Japanese,
    /// or Korean character, full-width form, or emoji, as terminals lay
    /// them out, and one for anything else.
    public static func cells(_ text: String) -> Int {
        text.reduce(0) { $0 + cells($1) }
    }

    static func cells(_ character: Character) -> Int {
        guard let first = character.unicodeScalars.first else { return 0 }
        if first.properties.isEmojiPresentation || character.unicodeScalars.contains("\u{FE0F}") { return 2 }
        return wide.contains { $0.contains(first.value) } ? 2 : 1
    }

    /// East Asian Wide and Fullwidth ranges outside emoji.
    private static let wide: [ClosedRange<UInt32>] = [
        0x1100...0x115F, // Hangul Jamo leading consonants
        0x2E80...0x303E, // CJK radicals, symbols, and punctuation
        0x3041...0x33FF, // kana, Bopomofo, Hangul compatibility Jamo, CJK compatibility
        0x3400...0x4DBF, // CJK extension A
        0x4E00...0x9FFF, // CJK unified ideographs
        0xA000...0xA4CF, // Yi
        0xA960...0xA97F, // Hangul Jamo extended A
        0xAC00...0xD7A3, // Hangul syllables
        0xF900...0xFAFF, // CJK compatibility ideographs
        0xFE10...0xFE19, // vertical forms
        0xFE30...0xFE6F, // CJK compatibility and small forms
        0xFF00...0xFF60, // fullwidth forms
        0xFFE0...0xFFE6, // fullwidth signs
        0x20000...0x3FFFD, // CJK extensions B and later
    ]
}
