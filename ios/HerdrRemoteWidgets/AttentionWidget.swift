import SwiftUI
import WidgetKit

/// The needs-you widget: how many chats wait on you and how many run,
/// across every paired computer, from the summary the app keeps
/// (`WidgetSummary`). The medium size lists the chats, each a link into
/// the app; the lock screen sizes show the counts.
struct AttentionWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: WidgetSummary.kind, provider: Provider()) { entry in
            AttentionView(entry: entry)
                .containerBackground(.fill.tertiary, for: .widget)
        }
        .configurationDisplayName("Needs You")
        .description("Chats waiting on you and running, on every computer.")
        .supportedFamilies([.systemSmall, .systemMedium, .accessoryCircular, .accessoryRectangular, .accessoryInline])
    }

    struct Entry: TimelineEntry {
        let date: Date
        let summary: WidgetSummary
    }

    /// The app reloads the timeline when the summary changes; between
    /// those, there is nothing newer to draw.
    struct Provider: TimelineProvider {
        func placeholder(in context: Context) -> Entry {
            Entry(date: .now, summary: .sample)
        }

        func getSnapshot(in context: Context, completion: @escaping (Entry) -> Void) {
            completion(Entry(date: .now, summary: context.isPreview ? .sample : WidgetSummary.load()))
        }

        func getTimeline(in context: Context, completion: @escaping (Timeline<Entry>) -> Void) {
            completion(Timeline(entries: [Entry(date: .now, summary: WidgetSummary.load())], policy: .never))
        }
    }
}

private struct AttentionView: View {
    @Environment(\.widgetFamily) private var family
    let entry: AttentionWidget.Entry

    private var summary: WidgetSummary { entry.summary }

    var body: some View {
        switch family {
        case .accessoryCircular:
            ZStack {
                AccessoryWidgetBackground()
                VStack(spacing: 0) {
                    Text("\(summary.waiting)").font(.title2.weight(.bold)).widgetAccentable()
                    Image(systemName: "hand.raised.fill").font(.caption2)
                }
            }
            .accessibilityLabel("\(summary.waiting) chats need you")
        case .accessoryInline:
            Text(inline)
        case .accessoryRectangular:
            VStack(alignment: .leading, spacing: 2) {
                Text(inline).font(.headline).widgetAccentable()
                if let first = summary.ranked.first {
                    // Chat titles stay off a locked phone's screen.
                    Text(first.title).font(.caption).lineLimit(1).privacySensitive()
                    Text("\(first.computer) · \(first.workspace)").font(.caption2).foregroundStyle(.secondary).lineLimit(1).privacySensitive()
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        case .systemMedium:
            HStack(alignment: .top, spacing: 14) {
                counts.frame(width: 96, alignment: .leading)
                VStack(alignment: .leading, spacing: 6) {
                    if summary.ranked.isEmpty {
                        Text("Nothing running").font(.subheadline).foregroundStyle(.secondary)
                    }
                    ForEach(summary.ranked.prefix(4)) { chat in
                        Link(destination: chat.link) { ChatLine(chat: chat) }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        default:
            counts
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .widgetURL(summary.ranked.first(where: \.needsYou)?.link)
        }
    }

    private var inline: String {
        switch (summary.waiting, summary.working) {
        case (0, 0): "Herdr: all quiet"
        case let (0, working): "Herdr: \(working) running"
        case let (waiting, _): "Herdr: \(waiting) need\(waiting == 1 ? "s" : "") you"
        }
    }

    private var counts: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label("Herdr", systemImage: "rectangle.3.group").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            HStack(alignment: .firstTextBaseline, spacing: 4) {
                Text("\(summary.waiting)").font(.system(size: 38, weight: .bold, design: .rounded))
                    .foregroundStyle(summary.waiting > 0 ? Palette.question : Color.primary)
                Text("need you").font(.caption)
            }
            Label("\(summary.working) running", systemImage: "circle.dotted")
                .font(.caption).foregroundStyle(summary.working > 0 ? Palette.working : Color.secondary)
            Spacer(minLength: 0)
            Text(entry.summary.updated, style: .relative).font(.caption2).foregroundStyle(.tertiary)
        }
    }
}

private struct ChatLine: View {
    let chat: WidgetSummary.Chat

    var body: some View {
        HStack(spacing: 6) {
            Circle().fill(chat.needsYou ? Palette.question : chat.working ? Palette.working : Color.secondary)
                .frame(width: 7, height: 7)
            VStack(alignment: .leading, spacing: 0) {
                Text(chat.title).font(.caption.weight(.medium)).lineLimit(1).privacySensitive()
                Text("\(chat.computer) · \(chat.workspace)").font(.caption2).foregroundStyle(.secondary).lineLimit(1).privacySensitive()
            }
        }
    }
}

extension WidgetSummary {
    /// Made-up chats for the widget gallery.
    static let sample = WidgetSummary(
        chats: [
            .init(device: "d", computerId: "", computer: "Studio", pane: "p1", title: "Fix mise discovery", workspace: "herdr", workspaceId: "w1", agent: "claude", needsYou: true, working: false, question: nil),
            .init(device: "d", computerId: "", computer: "Studio", pane: "p2", title: "Equalize splits", workspace: "herdr", workspaceId: "w1", agent: "codex", needsYou: false, working: true, question: nil),
        ],
        updated: .now
    )
}
