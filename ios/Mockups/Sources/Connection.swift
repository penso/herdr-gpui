import SwiftUI

// How Home says a computer is not live. State 0: just opened, cached
// chats shown while the link comes back (usually under a second).
// State 1: the computer has not answered for a while.
// Now: today's banner, with the list dimmed. A: the title's subtitle,
// like Mail's "Updated just now". B: a status capsule in the toolbar.
// C: a capsule above the tab bar that leaves once connected. D: nothing
// at first; a card only once the computer has been gone a while.

private let computer = "MacBook Pro"

/// Home B in its tab bar, with what each variant adds around it.
private struct ConnectionHome<Extra: ToolbarContent>: View {
    var subtitle: Text?
    var dimmed = false
    var banner: AnyView?
    var bottom: AnyView?
    @ToolbarContentBuilder var extra: () -> Extra

    var body: some View {
        TabView {
            Tab("Home", systemImage: "house") {
                NavigationStack {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 14) {
                            if let banner { banner }
                            AttentionBody().opacity(dimmed ? 0.6 : 1)
                        }
                        .padding(.horizontal, 16)
                    }
                    .background(Color(.systemGroupedBackground))
                    .navigationTitle("Herdr")
                    .navigationSubtitle(subtitle ?? Text(""))
                    .toolbar {
                        extra()
                        ToolbarItem(placement: .topBarTrailing) {
                            Button {} label: { Image(systemName: "plus") }
                        }
                    }
                    .overlay(alignment: .bottom) {
                        if let bottom { bottom.padding(.bottom, 12) }
                    }
                }
            }
            Tab("Board", systemImage: "square.grid.2x2") { Text("Board") }
            Tab("Computers", systemImage: "desktopcomputer") { Text("Computers") }
        }
    }
}

/// Today: a glass banner at the top, the list at 60%.
struct ConnectionNow: View {
    var state: Int

    var body: some View {
        ConnectionHome(
            dimmed: true,
            banner: AnyView(
                Text(state == 0 ? "Reconnecting to \(computer)…" : "\(computer): The computer did not answer.")
                    .font(.footnote).foregroundStyle(.secondary)
                    .padding(.horizontal, 14).padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .glassEffect(.regular, in: .rect(cornerRadius: 16))
            )
        ) { ToolbarSpacer(.fixed, placement: .topBarLeading) }
    }
}

/// A: under the title, the way Mail says it is checking. Nothing moves
/// and nothing dims; offline turns the line orange with the time shown.
struct ConnectionSubtitle: View {
    var state: Int

    var body: some View {
        ConnectionHome(
            subtitle: state == 0
                ? Text("Updating…")
                : Text("\(Image(systemName: "exclamationmark.circle.fill")) \(computer) offline · as of 9:38").foregroundStyle(.orange)
        ) { ToolbarSpacer(.fixed, placement: .topBarLeading) }
    }
}

/// B: a capsule in the toolbar, a spinning ring while connecting and a
/// dot after; tapping it lists every computer's link.
struct ConnectionCapsule: View {
    var state: Int

    var body: some View {
        ConnectionHome {
            ToolbarItem(placement: .topBarLeading) {
                Button {} label: {
                    HStack(spacing: 6) {
                        if state == 0 {
                            ProgressView().controlSize(.mini)
                        } else {
                            Circle().fill(.orange).frame(width: 8, height: 8)
                        }
                        Text(state == 0 ? "Connecting" : "Offline").font(.subheadline.weight(.medium))
                    }
                }
            }
        }
    }
}

/// C: above the tab bar, like Music's "Playing on"; it slides away once
/// connected, and stays with Retry when the computer is gone.
struct ConnectionToast: View {
    var state: Int

    var body: some View {
        ConnectionHome(bottom: AnyView(toast)) { ToolbarSpacer(.fixed, placement: .topBarLeading) }
    }

    private var toast: some View {
        HStack(spacing: 10) {
            if state == 0 {
                ProgressView().controlSize(.small)
                Text("Connecting to \(computer)").font(.subheadline.weight(.medium))
            } else {
                Image(systemName: "wifi.slash").foregroundStyle(.orange)
                VStack(alignment: .leading, spacing: 0) {
                    Text("\(computer) is offline").font(.subheadline.weight(.medium))
                    Text("Showing chats as of 9:38").font(.caption).foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
                Button("Retry") {}.buttonStyle(.glass).controlSize(.small)
            }
        }
        .padding(.horizontal, 16).padding(.vertical, 10)
        .frame(maxWidth: state == 0 ? nil : .infinity)
        .glassEffect(.regular, in: .capsule)
        .padding(.horizontal, 16)
    }
}

/// D: says nothing for the first seconds, since the link is usually back
/// before anyone reads a banner; after that, one card that explains and
/// offers Retry. The list never dims.
struct ConnectionQuiet: View {
    var state: Int

    var body: some View {
        ConnectionHome(banner: state == 0 ? nil : AnyView(card)) { ToolbarSpacer(.fixed, placement: .topBarLeading) }
    }

    private var card: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "laptopcomputer.slash")
                .font(.title3).foregroundStyle(.orange)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 4) {
                Text("Can't reach \(computer)").font(.subheadline.weight(.semibold))
                Text("Chats below are from 9:38. Herdr keeps trying; answers wait until it is back.")
                    .font(.footnote).foregroundStyle(.secondary)
                HStack(spacing: 8) {
                    Button("Retry") {}.buttonStyle(.glassProminent).controlSize(.small)
                    Button("Details") {}.buttonStyle(.glass).controlSize(.small)
                }
                .padding(.top, 4)
            }
            Spacer(minLength: 0)
        }
        .padding(14)
        .background(.orange.opacity(0.1), in: .rect(cornerRadius: 18))
    }
}
