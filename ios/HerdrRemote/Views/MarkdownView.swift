import HerdrRemoteCore
import SwiftUI
import UIKit

/// An agent's answer as native blocks (`MarkdownBlock`): code scrolls
/// sideways with Copy, tables scroll as a grid, and a long press copies a
/// block. Blocks keep their positions as text streams in, so appended text
/// grows the last block without reflowing the ones above it.
struct MarkdownView: View {
    let text: String

    var body: some View {
        let blocks = MarkdownBlock.parse(text)
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                BlockView(block: block)
            }
        }
    }

    /// Inline markdown from the agent, which is untrusted: only `http` and
    /// `https` links stay links (opening one asks first, see ChatScreen),
    /// so transcript text can never follow the app's own scheme.
    static func inline(_ text: String) -> AttributedString {
        guard var styled = try? AttributedString(markdown: text, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))
        else { return AttributedString(text) }
        for run in styled.runs {
            if let link = run.link, !webSchemes.contains(link.scheme?.lowercased() ?? "") {
                styled[run.range].link = nil
            }
        }
        return styled
    }

    static let webSchemes: Set<String> = ["http", "https"]
}

private struct BlockView: View {
    let block: MarkdownBlock

    var body: some View {
        switch block {
        case let .paragraph(text):
            Text(MarkdownView.inline(text)).textSelection(.enabled).copyable(text)
        case let .heading(level, text):
            Text(MarkdownView.inline(text)).font(font(level)).padding(.top, level <= 2 ? 6 : 2)
                .accessibilityAddTraits(.isHeader)
        case let .code(language, text):
            CodeBlock(language: language, text: text)
        case let .list(ordered, start, items):
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        marker(ordered: ordered, number: start + index, item: item)
                        Text(MarkdownView.inline(item.text)).textSelection(.enabled)
                    }
                    .padding(.leading, CGFloat(item.depth) * 18)
                }
            }
            .copyable(items.map(\.text).joined(separator: "\n"))
        case let .quote(text):
            HStack(spacing: 10) {
                RoundedRectangle(cornerRadius: 1.5).fill(.quaternary).frame(width: 3)
                Text(MarkdownView.inline(text)).foregroundStyle(.secondary).textSelection(.enabled)
            }
            .fixedSize(horizontal: false, vertical: true)
        case let .table(header, rows):
            TableBlock(header: header, rows: rows)
        case .rule:
            Divider().padding(.vertical, 4)
        }
    }

    private func font(_ level: Int) -> Font {
        switch level {
        case 1: .title2.weight(.bold)
        case 2: .title3.weight(.semibold)
        case 3: .headline
        default: .subheadline.weight(.semibold)
        }
    }

    @ViewBuilder
    private func marker(ordered: Bool, number: Int, item: MarkdownListItem) -> some View {
        if let checked = item.checked {
            Image(systemName: checked ? "checkmark.square.fill" : "square")
                .foregroundStyle(checked ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                .accessibilityLabel(checked ? "Done" : "To do")
        } else if ordered {
            Text("\(number).").monospacedDigit().foregroundStyle(.secondary)
        } else {
            Text(item.depth == 0 ? "•" : "◦").foregroundStyle(.secondary)
        }
    }
}

/// Code that scrolls sideways rather than wrapping, with its language and
/// a Copy button.
struct CodeBlock: View {
    let language: String?
    let text: String
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language ?? "code").font(.caption.monospaced()).foregroundStyle(.secondary)
                Spacer()
                Button(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc") {
                    UIPasteboard.general.string = text
                    copied = true
                    Task {
                        try? await Task.sleep(for: .seconds(1.5))
                        copied = false
                    }
                }
                .font(.caption)
                .labelStyle(.titleAndIcon)
                .sensoryFeedback(.success, trigger: copied) { _, now in now }
            }
            .padding(.horizontal, 12).padding(.top, 8).padding(.bottom, 4)
            ScrollView(.horizontal, showsIndicators: false) {
                Text(text).font(.system(.footnote, design: .monospaced)).textSelection(.enabled)
                    .fixedSize(horizontal: true, vertical: false)
                    .padding(.horizontal, 12).padding(.bottom, 10)
            }
        }
        .background(.fill.quinary, in: .rect(cornerRadius: 12))
    }
}

private struct TableBlock: View {
    let header: [String]
    let rows: [[String]]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 6) {
                GridRow {
                    ForEach(Array(header.enumerated()), id: \.offset) { _, cell in
                        Text(MarkdownView.inline(cell)).font(.subheadline.weight(.semibold))
                    }
                }
                Divider()
                ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                    GridRow {
                        ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
                            Text(MarkdownView.inline(cell)).font(.subheadline)
                        }
                    }
                }
            }
            .padding(12)
        }
        .background(.fill.quinary, in: .rect(cornerRadius: 12))
        .copyable(([header] + rows).map { $0.joined(separator: "\t") }.joined(separator: "\n"))
    }
}

extension View {
    /// A long press offers to copy `text`, the block's source.
    func copyable(_ text: String) -> some View {
        contextMenu {
            Button("Copy", systemImage: "doc.on.doc") { UIPasteboard.general.string = text }
        }
    }
}
