import HerdrRemoteCore
import SwiftUI
import UIKit

/// Connection C: a capsule above the tab bar that says which computers
/// are not live. Connecting shows only after a moment, since the link is
/// usually back before anyone could read it; it slides away once live.
///
/// It never stands in the way: it hides while the keyboard is up and on
/// screens with their own input at the bottom (chats, terminals), and its
/// close button keeps it away for those computers until each is online
/// again (`LinkNotice`). Links retry by themselves, sooner when the network
/// changes or the app comes back, so it offers no Retry.
struct ConnectionToast: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?
    @Binding var pairing: Bool
    /// Whether connecting has lasted long enough to mention.
    @State private var patient = false
    @State private var keyboard = false

    /// How long a link may connect before the toast says so.
    static let grace: Duration = .seconds(1.5)

    var body: some View {
        let machines = model.fleet.machines
        let connecting = machines.filter { $0.status == .connecting }.map(\.id)
        let online = Set(machines.filter { if case .online = $0.status { true } else { false } }.map(\.id))
        let notice = keyboard || (window?.typing ?? 0) > 0 ? nil : LinkNotice.worst(
            machines.map { ($0.id, $0.status) },
            patient: patient,
            dismissed: model.dismissedNotices,
            // Home's cards ask about unpaired computers instead.
            skipping: window?.home == true ? [.revoked] : []
        )
        Group {
            if let notice {
                toast(notice, machines: machines)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.spring(duration: 0.35), value: notice)
        .task(id: connecting) {
            patient = false
            guard !connecting.isEmpty else { return }
            try? await Task.sleep(for: Self.grace)
            patient = !Task.isCancelled
        }
        .onChange(of: online) { _, online in
            model.dismissedNotices = LinkNotice.cleared(model.dismissedNotices, online: online)
        }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillShowNotification)) { _ in keyboard = true }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in keyboard = false }
    }

    @ViewBuilder
    private func toast(_ notice: LinkNotice, machines: [Fleet.Machine]) -> some View {
        let named = machines.filter { notice.devices.contains($0.id) }
        let names = named.map { model.displayName($0.computer) }
        let close = { model.dismissedNotices.formUnion(notice.keys) }
        switch notice.kind {
        case .connecting:
            capsule(close: close) {
                ProgressView().controlSize(.small)
                Text("Connecting to \(Self.list(names))").font(.subheadline.weight(.medium)).lineLimit(1)
            }
        case .offline:
            // The oldest view still shown is the one worth dating.
            let since = named.compactMap(\.updated).min()
            let reason = named.lazy.compactMap { if case let .offline(reason) = $0.status { reason } else { nil } }.first ?? ""
            capsule(close: close) {
                Image(systemName: "wifi.slash").foregroundStyle(.orange)
                VStack(alignment: .leading, spacing: 0) {
                    Text(names.count == 1 ? "\(names[0]) is offline" : "\(names.count) computers are offline")
                        .font(.subheadline.weight(.medium)).lineLimit(1)
                    Text(since.map { "Showing chats as of \(Self.time($0))" } ?? reason)
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
        case .outdated:
            capsule(close: close) {
                Image(systemName: "arrow.up.circle").foregroundStyle(.orange)
                VStack(alignment: .leading, spacing: 0) {
                    Text(names.count == 1 ? "\(names[0]) needs a newer Herdr" : "\(names.count) computers need a newer Herdr")
                        .font(.subheadline.weight(.medium)).lineLimit(1)
                    Text("Update Herdr there to see its chats").font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
        case .revoked:
            capsule(close: close) {
                Image(systemName: "laptopcomputer.slash").foregroundStyle(.red)
                VStack(alignment: .leading, spacing: 0) {
                    Text("\(Self.list(names)) unpaired this phone").font(.subheadline.weight(.medium)).lineLimit(1)
                    Text("Pair again to see its chats").font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                Button("Pair") { pairing = true }
                    .buttonStyle(.glass).controlSize(.small)
            }
        }
    }

    private func capsule(close: @escaping () -> Void, @ViewBuilder content: () -> some View) -> some View {
        HStack(spacing: 10) {
            content()
            Spacer(minLength: 8)
            Button(action: close) {
                Image(systemName: "xmark").font(.caption.weight(.bold)).frame(width: 28, height: 28)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .accessibilityLabel("Close")
        }
        .padding(.leading, 16).padding(.trailing, 8).padding(.vertical, 8)
        .frame(maxWidth: .infinity)
        .glassEffect(.regular, in: .capsule)
        .padding(.horizontal, 16).padding(.bottom, 8)
    }

    static func list(_ names: [String]) -> String {
        names.count == 1 ? names[0] : "\(names.count) computers"
    }

    /// The time alone today, the day and time before.
    static func time(_ date: Date) -> String {
        Calendar.current.isDateInToday(date)
            ? date.formatted(date: .omitted, time: .shortened)
            : date.formatted(.dateTime.month(.abbreviated).day().hour().minute())
    }
}

extension View {
    /// Says at the bottom which computers are not live, on this view and
    /// on every screen a navigation stack it wraps pushes.
    func connectionToast(pairing: Binding<Bool>) -> some View {
        safeAreaInset(edge: .bottom) { ConnectionToast(pairing: pairing) }
    }

    /// For a screen with its own input at the bottom: its window's
    /// connection toast stays away while it shows.
    func hidesConnectionToast() -> some View {
        modifier(HidesConnectionToast())
    }
}

private struct HidesConnectionToast: ViewModifier {
    @Environment(WindowState.self) private var window: WindowState?

    func body(content: Content) -> some View {
        content
            .onAppear { window?.typing += 1 }
            .onDisappear { window.map { $0.typing = max(0, $0.typing - 1) } }
    }
}
