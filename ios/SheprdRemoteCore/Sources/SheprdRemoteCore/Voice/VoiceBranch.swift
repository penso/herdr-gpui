import Foundation

/// A branch name for what a worktree is about, as a person would write it:
/// "the login redirect bug" is `fix/login-redirect`.
public enum VoiceBranch {
    private static let fixes: Set<String> = ["bug", "bugs", "fix", "fixes", "crash", "crashes", "broken", "issue", "regression"]
    private static let dropped: Set<String> = ["the", "a", "an", "my", "our", "some", "this", "that", "of", "to", "for", "thing", "stuff", "about"]
    private static let longest = 40

    /// Whether `topic` shares words with `task` and has some it does not:
    /// a topic about the same work with a word that may be misheard. A
    /// topic sharing nothing is just another way of saying it, left alone.
    public static func doubtful(topic: String, task: String) -> Bool {
        let said = Set(VoiceNames.words(task).map(\.text))
        let meaning = VoiceNames.words(topic).map(\.text).filter { !dropped.contains($0) && !fixes.contains($0) }
        return meaning.contains(where: said.contains) && !meaning.allSatisfy(said.contains)
    }

    /// `name` for tidied words, keeping the kind the original topic gave.
    public static func name(for words: String, topic: String) -> String? {
        let fixing = VoiceNames.words(topic).map(\.text).contains(where: fixes.contains)
        return name(for: fixing ? "\(words) fix" : words)
    }

    public static func name(for topic: String) -> String? {
        let words = VoiceNames.words(topic).map(\.text)
        let meaning = words.filter { !dropped.contains($0) && !fixes.contains($0) }
        let safe = meaning.map { $0.filter { $0.isASCII && ($0.isLetter || $0.isNumber) } }.filter { !$0.isEmpty }
        guard !safe.isEmpty else { return nil }
        var slug = ""
        for word in safe {
            guard slug.count + word.count < longest else { break }
            slug += slug.isEmpty ? word : "-\(word)"
        }
        guard !slug.isEmpty else { return nil }
        return (words.contains(where: fixes.contains) ? "fix/" : "feature/") + slug
    }
}
