import HerdrRemoteCore
import SwiftUI
import UIKit

/// One transcript entry.
struct EntryView: View {
    let entry: Entry

    var body: some View {
        switch entry.kind {
        case let .user(text, _):
            Text(text).padding(12).frame(maxWidth: .infinity, alignment: .leading)
                .background(.tint.opacity(0.1), in: .rect(cornerRadius: 16))
                .textSelection(.enabled)
        case let .agent(text):
            MarkdownView(text: text)
        case let .thought(text):
            Text(text).font(.callout.italic()).foregroundStyle(.secondary).lineLimit(4)
        case let .image(_, image):
            if let data = Data(base64Encoded: image.data), let picture = UIImage(data: data) {
                ChatPicture(image: picture)
            }
        case let .tool(tool):
            ToolRow(tool: tool)
        case let .turn(summary):
            Text(turnLine(summary)).font(.caption).foregroundStyle(.secondary)
        case let .notice(notice):
            Label(notice.text, systemImage: notice.level == .error ? "exclamationmark.octagon" : "info.circle")
                .font(.footnote)
                .foregroundStyle(notice.level == .info ? AnyShapeStyle(.secondary) : AnyShapeStyle(notice.level == .error ? .red : .orange))
        }
    }

    private func turnLine(_ summary: TurnSummary) -> String {
        let seconds = Int(summary.durationMs / 1000)
        let time = seconds >= 60 ? "\(seconds / 60)m \(seconds % 60)s" : "\(seconds)s"
        return summary.end == .done ? "Done in \(time)" : "Ended (\(summary.end.rawValue)) after \(time)"
    }
}

struct ToolRow: View {
    let tool: Tool

    var body: some View {
        HStack(spacing: 10) {
            Group {
                switch tool.status {
                case .inProgress: ProgressView().controlSize(.mini)
                case .failed: Image(systemName: "xmark.circle.fill").foregroundStyle(.red)
                case .completed: Image(systemName: symbol)
                case .pending: Image(systemName: "circle.dotted").foregroundStyle(.tertiary)
                }
            }
            .frame(width: 20)
            Text(tool.title).font(.subheadline).lineLimit(2)
        }
    }

    private var symbol: String {
        switch tool.kind {
        case .read: "doc.text.magnifyingglass"
        case .edit: "pencil"
        case .delete: "trash"
        case .move: "arrow.right.doc.on.clipboard"
        case .search: "magnifyingglass"
        case .execute: "terminal"
        case .think: "brain"
        case .fetch: "globe"
        case .switchMode: "arrow.triangle.swap"
        case .other: "wrench.and.screwdriver"
        }
    }
}

/// A file change, folded to its name and line counts until tapped, then
/// its removed and added lines, scrolling sideways.
struct DiffBlock: View {
    let path: String
    let old: String?
    let new: String

    /// Folded to one line at first: on a phone, what the agent said matters
    /// more than the code, which is a tap away.
    @State private var expanded = false

    var body: some View {
        let lines = lines
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(.snappy) { expanded.toggle() }
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: "doc.text").foregroundStyle(.secondary)
                    Text((path as NSString).lastPathComponent).font(.caption.monospaced().weight(.semibold))
                        .lineLimit(1).truncationMode(.middle)
                    Spacer(minLength: 4)
                    let added = lines.count { $0.mark == "+" }
                    let removed = lines.count - added
                    if added > 0 { Text("+\(added)").foregroundStyle(.green) }
                    if removed > 0 { Text("−\(removed)").foregroundStyle(.red) }
                    Image(systemName: "chevron.right").foregroundStyle(.secondary)
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                }
                .font(.caption.monospaced())
                .padding(.horizontal, 12).padding(.vertical, 10)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(path), \(expanded ? "hide" : "show") changes")
            if expanded { changes(lines) }
        }
        .background(.fill.quinary, in: .rect(cornerRadius: 12))
    }

    @ViewBuilder
    private func changes(_ lines: [(mark: String, text: String)]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(path).font(.caption2.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.head)
                .padding(.horizontal, 12).padding(.bottom, 6)
            ScrollView(.horizontal, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(lines.prefix(60).enumerated()), id: \.offset) { _, line in
                        Text("\(line.mark) \(line.text)")
                            .font(.system(.caption, design: .monospaced))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 12)
                            .background(line.mark == "+" ? Color.green.opacity(0.18) : line.mark == "-" ? Color.red.opacity(0.18) : .clear)
                    }
                }
            }
            if lines.count > 60 {
                Text("\(lines.count - 60) more lines").font(.caption).foregroundStyle(.secondary).padding(12)
            }
        }
        .padding(.bottom, 8)
        .transition(.opacity)
    }

    /// Removed lines then added ones: the change, not a minimal diff.
    private var lines: [(mark: String, text: String)] {
        let before = (old ?? "").split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        let after = new.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        let common = Set(before).intersection(after)
        return before.filter { !common.contains($0) }.map { ("-", $0) } + after.filter { !common.contains($0) }.map { ("+", $0) }
    }
}
