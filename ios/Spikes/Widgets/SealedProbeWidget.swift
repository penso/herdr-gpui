import ActivityKit
import SwiftUI
import WidgetKit

/// Draws what the sealed state opened to, or why it did not open, so the
/// lock screen itself is the spike's result.
struct SealedProbeLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: SealedProbeAttributes.self) { context in
            let report = SealedProbe.open(context.state)
            VStack(alignment: .leading, spacing: 4) {
                Text(context.attributes.label).font(.caption).foregroundStyle(.secondary)
                Text(report.headline).font(.headline)
                Text(report.detail).font(.caption)
            }
            .padding()
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.bottom) {
                    Text(SealedProbe.open(context.state).headline)
                }
            } compactLeading: {
                Image(systemName: "lock")
            } compactTrailing: {
                Text("…")
            } minimal: {
                Image(systemName: "lock")
            }
        }
    }
}
