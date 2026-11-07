import Foundation

/// A branch name for what a worktree is about, as a person would write it:
/// "the login redirect bug" is `fix/login-redirect`.
public enum VoiceBranch {
    private static let fixes: Set<String> = ["bug", "bugs", "fix", "fixes", "crash", "crashes", "broken", "issue", "regression"]
    private static let dropped: Set<String> = ["the", "a", "an", "my", "our", "some", "this", "that", "of", "to", "for", "thing", "stuff", "about"]
    private static let longest = 40

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
