import HerdrRemoteCore
import SwiftUI

/// A message search found: its chat, who said it, and the words around
/// the match with the match in bold.
struct MessageHitRow: View {
    let hit: MessageHit
    let query: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                Image(systemName: hit.hit.author == .user ? "person.fill" : "sparkles")
                    .font(.caption).foregroundStyle(.secondary)
                    .accessibilityLabel(hit.hit.author == .user ? "You" : "Agent")
                Text(hit.item.chat.title).font(.subheadline.weight(.semibold)).lineLimit(1)
                Spacer(minLength: 4)
                if hit.local {
                    Image(systemName: "iphone").font(.caption2).foregroundStyle(.tertiary)
                        .accessibilityLabel("From this phone")
                }
                Text(hit.item.computer.name).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Text(highlighted).font(.subheadline).foregroundStyle(.secondary).lineLimit(3)
        }
        .accessibilityElement(children: .combine)
    }

    private var highlighted: AttributedString {
        var text = AttributedString(hit.hit.snippet)
        if let range = text.range(of: MessageText.line(query), options: .caseInsensitive) {
            text[range].foregroundColor = .primary
            text[range].font = .subheadline.weight(.semibold)
        }
        return text
    }
}
