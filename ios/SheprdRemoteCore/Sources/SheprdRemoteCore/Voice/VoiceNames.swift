import Foundation

/// A word of what was said, and where it sits in the text.
struct VoiceWord: Hashable, Sendable {
    /// Lowercased, with spoken digits read as digits.
    var text: String
    var range: Range<String.Index>
}

/// Where a name was heard, and how sure that is.
struct VoiceSpot: Hashable, Sendable {
    var range: Range<String.Index>
    /// The first and one past the last word it covers.
    var words: Range<Int>
    var score: Double
}

/// Finding the names of computers, projects, and chats in speech. The
/// transcriber has never seen most of them: "sheprd" comes back as
/// "shepherd", and "build-01" as "build oh one".
enum VoiceNames {
    /// From here a stretch of speech counts as a name.
    static let accepted = 0.72

    private static let digits = [
        "zero": "0", "oh": "0", "one": "1", "two": "2", "three": "3", "four": "4",
        "five": "5", "six": "6", "seven": "7", "eight": "8", "nine": "9",
    ]

    /// The words of `text`. A run of spoken digits is one word, so
    /// "oh one" is "01". A lone "oh" or "one" stays a word.
    static func words(_ text: String) -> [VoiceWord] {
        var plain: [VoiceWord] = []
        var start: String.Index?
        var index = text.startIndex
        while index < text.endIndex {
            let next = text.index(after: index)
            let kept = text[index].isLetter || text[index].isNumber
                || ((text[index] == "'" || text[index] == "’") && start != nil)
            if kept {
                if start == nil { start = index }
            } else if let from = start {
                plain.append(VoiceWord(text: folded(text[from..<index]), range: from..<index))
                start = nil
            }
            index = next
        }
        if let start { plain.append(VoiceWord(text: folded(text[start...]), range: start..<text.endIndex)) }
        return mergingCompounds(mergingDigits(plain))
    }

    /// The transcriber writes these as two words.
    private static func mergingCompounds(_ words: [VoiceWord]) -> [VoiceWord] {
        var merged: [VoiceWord] = []
        for word in words {
            if let last = merged.last, last.text == "work", ["tree", "space"].contains(word.text) {
                merged[merged.count - 1] = VoiceWord(text: last.text + word.text, range: last.range.lowerBound..<word.range.upperBound)
            } else {
                merged.append(word)
            }
        }
        return merged
    }

    /// The transcriber writes a curly apostrophe, typed text a straight one.
    private static func folded(_ word: Substring) -> String {
        word.lowercased().replacingOccurrences(of: "’", with: "'")
    }

    private static func mergingDigits(_ words: [VoiceWord]) -> [VoiceWord] {
        var merged: [VoiceWord] = []
        var index = 0
        while index < words.count {
            var end = index
            var number = ""
            while end < words.count, let digit = digit(words[end].text) {
                number += digit
                end += 1
            }
            if end - index >= 2 {
                merged.append(VoiceWord(text: number, range: words[index].range.lowerBound..<words[end - 1].range.upperBound))
                index = end
            } else {
                merged.append(words[index])
                index += 1
            }
        }
        return merged
    }

    private static func digit(_ word: String) -> String? {
        digits[word] ?? (word.allSatisfy(\.isNumber) ? word : nil)
    }

    /// Roughly how `word` sounds: what speech confuses is alike here.
    static func sound(_ word: String) -> String {
        var letters = word.lowercased()
        for (from, to) in [("ph", "f"), ("ck", "k"), ("qu", "k"), ("x", "ks"), ("z", "s"), ("q", "k")] {
            letters = letters.replacingOccurrences(of: from, with: to)
        }
        var key = ""
        for (offset, letter) in letters.enumerated() {
            var sound = letter
            if letter == "c" { sound = "k" }
            let vowel = "aeiouy".contains(letter)
            if vowel, offset > 0 { continue }
            if key.last != sound { key.append(sound) }
        }
        return key
    }

    /// Single-character edits between `a` and `b`.
    static func distance(_ a: String, _ b: String) -> Int {
        let a = Array(a), b = Array(b)
        guard !a.isEmpty else { return b.count }
        guard !b.isEmpty else { return a.count }
        var row = Array(0...b.count)
        for i in 1...a.count {
            var previous = row[0]
            row[0] = i
            for j in 1...b.count {
                let held = row[j]
                row[j] = min(row[j] + 1, row[j - 1] + 1, previous + (a[i - 1] == b[j - 1] ? 0 : 1))
                previous = held
            }
        }
        return row[b.count]
    }

    /// How alike two spellings are, from 0 to 1. Words are compared without
    /// the spaces between them, as speech may split or join a name.
    static func likeness(_ heard: String, _ name: String) -> Double {
        guard !heard.isEmpty, !name.isEmpty else { return 0 }
        if heard == name { return 1 }
        // A plural is the same name, and short names need exact sounds.
        if heard == name + "s" { return 0.92 }
        let spelled = 1 - Double(distance(heard, name)) / Double(max(heard.count, name.count))
        let a = sound(heard), b = sound(name)
        guard a.first == b.first else { return spelled >= 0.8 ? spelled * 0.9 : 0 }
        if a == b, min(heard.count, name.count) >= 4 { return 0.9 }
        if min(a.count, b.count) >= 4, distance(a, b) == 1 { return 0.8 }
        return spelled >= 0.8 ? spelled * 0.9 : 0
    }

    /// The best place `name` was heard among `words`. With `partly`, a part
    /// of a long name counts, such as "macbook" for "MacBook Pro".
    static func find(_ name: String, in words: [VoiceWord], partly: Bool = false) -> VoiceSpot? {
        let parts = Self.words(name).map(\.text)
        guard !parts.isEmpty else { return nil }
        let whole = parts.joined()
        var best: VoiceSpot?
        func consider(_ spot: VoiceSpot) {
            if spot.score >= accepted, spot.score > (best?.score ?? 0) { best = spot }
        }
        for start in words.indices {
            for length in 1...(parts.count + 2) where start + length <= words.count {
                let window = words[start..<(start + length)]
                let heard = window.map(\.text).joined()
                consider(VoiceSpot(
                    range: window.first!.range.lowerBound..<window.last!.range.upperBound,
                    words: start..<(start + length),
                    score: likeness(heard, whole)
                ))
            }
        }
        guard partly, parts.count > 1 else { return best }
        // Any run of a name's words, its first word counting most.
        for start in words.indices {
            for from in parts.indices {
                var length = 0
                while from + length < parts.count, start + length < words.count,
                      likeness(words[start + length].text, parts[from + length]) >= 0.85 {
                    length += 1
                }
                guard length > 0 else { continue }
                let distinctive = parts[from..<(from + length)].joined().count >= 4
                guard distinctive, !filler.contains(parts[from]) else { continue }
                let share = Double(length) / Double(parts.count)
                consider(VoiceSpot(
                    range: words[start].range.lowerBound..<words[start + length - 1].range.upperBound,
                    words: start..<(start + length),
                    score: (from == 0 ? 0.76 : 0.72) + share * 0.1
                ))
            }
        }
        return best
    }

    /// Whether `text` is `name` and nothing else.
    static func isAll(of text: String, _ name: String) -> Bool {
        let words = words(text)
        return find(name, in: words)?.words == 0..<words.count
    }

    /// Words a part of a name cannot be recognized by.
    private static let filler: Set<String> = ["the", "chat", "fix", "with", "from", "command", "pass", "this", "that", "pro"]
}
