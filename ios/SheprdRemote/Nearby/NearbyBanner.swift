import SheprdRemoteCore
import SwiftUI

/// A capsule at the top of Home and Workspaces when a computer nearby has
/// its pairing dialog open, for a phone that already has computers: it says
/// so without stopping what the person is doing. A tap opens the proximity
/// card, whose Not now closes the offer; closing the dialog there ends it
/// too, so it needs no close button.
private struct NearbyBanner: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?

    var body: some View {
        let offer = model.computers.isEmpty || window?.nearby != nil ? nil : model.nearbyOffer
        Group {
            if let offer {
                Button { window?.nearby = offer } label: { capsule(offer) }
                    .buttonStyle(.plain)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .animation(.spring(duration: 0.35), value: offer?.fingerprintPrefix)
    }

    private func capsule(_ offer: NearbyComputer) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "laptopcomputer").foregroundStyle(.tint)
            VStack(alignment: .leading, spacing: 0) {
                Text("\(offer.name) is ready to pair").font(.subheadline.weight(.medium)).lineLimit(1)
                Text(offer.routeName).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 8)
            Text("Pair").font(.subheadline.weight(.semibold)).foregroundStyle(.tint)
        }
        .padding(.horizontal, 16).padding(.vertical, 8)
        .frame(maxWidth: .infinity)
        .glassEffect(.regular, in: .capsule)
        .contentShape(.capsule)
        .padding(.horizontal, 16).padding(.bottom, 8)
    }
}

extension NearbyComputer {
    /// How the phone found it, as the person would say it.
    var routeName: String {
        if case .bluetooth = route { "Bluetooth" } else { "Same network" }
    }
}

/// The proximity card over a window's content, on its scrim.
private struct NearbyCard: ViewModifier {
    @Environment(AppModel.self) private var model
    let window: WindowState?
    let shown: Bool

    func body(content: Content) -> some View {
        let offer = shown ? window?.nearby : nil
        content
            .overlay(alignment: .bottom) {
                if let offer {
                    ZStack(alignment: .bottom) {
                        Color.black.opacity(0.25).ignoresSafeArea()
                        ProximityCard(computer: offer) {
                            model.dismissedNearby.insert(offer.fingerprintPrefix)
                            window?.nearby = nil
                        }
                        .id(offer.id)
                        .environment(window)
                    }
                }
            }
            .animation(.spring(duration: 0.4), value: offer?.id)
    }
}

extension View {
    /// Draws `window`'s proximity card here. Each window draws it in one
    /// place only: two would each run their own pairing.
    func nearbyCard(_ window: WindowState?, when shown: Bool = true) -> some View {
        modifier(NearbyCard(window: window, shown: shown))
    }

    /// Offers a computer ready to pair above this list, staying in view
    /// as the list scrolls.
    func nearbyBanner() -> some View {
        safeAreaInset(edge: .top) { NearbyBanner() }
    }
}
