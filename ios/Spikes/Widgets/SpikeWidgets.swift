import ActivityKit
import AppIntents
import SwiftUI
import WidgetKit

@main
struct SpikeWidgets: WidgetBundle {
    var body: some Widget {
        SpikeLiveActivity()
    }
}

/// A question with answer buttons on the lock screen; each runs
/// `SpikeAnswerIntent` without opening the app.
struct SpikeLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: SpikeActivityAttributes.self) { context in
            VStack(alignment: .leading, spacing: 8) {
                Text(context.attributes.chat).font(.caption).foregroundStyle(.secondary)
                Text(context.state.question).font(.headline)
                HStack {
                    ForEach(["Allow once", "Deny"], id: \.self) { option in
                        Button(intent: SpikeAnswerIntent(option: option)) {
                            Text(option).frame(maxWidth: .infinity)
                        }
                    }
                }
                if let result = context.state.result {
                    Text(result).font(.caption.monospaced())
                }
            }
            .padding()
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.bottom) {
                    HStack {
                        Button(intent: SpikeAnswerIntent(option: "Allow once")) { Text("Allow once") }
                        Button(intent: SpikeAnswerIntent(option: "Deny")) { Text("Deny") }
                    }
                }
            } compactLeading: {
                Image(systemName: "questionmark.circle")
            } compactTrailing: {
                Text(context.state.result == nil ? "?" : "✓")
            } minimal: {
                Image(systemName: "questionmark.circle")
            }
        }
    }
}
