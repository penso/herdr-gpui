import SwiftUI
import UIKit

/// The optional Face ID lock (Settings). While locked, a window above every
/// other one covers each of the app's windows, sheets and alerts included,
/// and in the app switcher, until the owner unlocks it. The setting lives
/// in the App Group, so the share extension honors it too.
struct AppLock: ViewModifier {
    @AppStorage(AppLock.setting, store: AppLock.store) private var required = false
    @Environment(\.scenePhase) private var phase
    private let state = LockState.shared

    nonisolated static let setting = "requireFaceID"
    nonisolated static var store: UserDefaults? { UserDefaults(suiteName: "group.so.pen.herdr.remote") }
    /// Fails closed: settings that cannot be read count as locked.
    nonisolated static var isOn: Bool { store?.bool(forKey: setting) ?? true }

    /// Moves a setting kept in the app's own defaults, before the App
    /// Group held it, so turning the lock on is never lost.
    static func migrate() {
        let old = UserDefaults.standard
        guard old.object(forKey: setting) != nil else { return }
        if old.bool(forKey: setting) { store?.set(true, forKey: setting) }
        old.removeObject(forKey: setting)
    }

    func body(content: Content) -> some View {
        content
            .onChange(of: required && (state.locked || phase != .active), initial: true) { _, covered in
                state.cover(covered)
            }
            .onChange(of: phase) { _, phase in
                if phase == .background, required { state.locked = true }
                if phase == .active, state.locked { Task { await state.unlock() } }
            }
            .onAppear { state.cover(required && (state.locked || phase != .active)) }
            .task { if state.locked { await state.unlock() } }
    }
}

/// One lock for every window of the app.
@MainActor
@Observable
final class LockState {
    static let shared = LockState()

    var locked = AppLock.isOn
    @ObservationIgnored private var covers: [ObjectIdentifier: UIWindow] = [:]
    @ObservationIgnored private var unlocking = false

    /// Whether chat names should stay hidden, as in window titles.
    var hidesNames: Bool { locked && AppLock.isOn }

    /// Covers every connected window scene, or none. Each window's lock
    /// calls this as it appears, so a window opened or restored while
    /// locked is covered too.
    func cover(_ covered: Bool) {
        guard covered else {
            covers.values.forEach { $0.isHidden = true }
            covers = [:]
            return
        }
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        // A cover whose scene went away; its identifier may be reused.
        for (id, window) in covers where window.windowScene == nil || !scenes.contains(where: { ObjectIdentifier($0) == id }) {
            window.isHidden = true
            covers[id] = nil
        }
        for scene in scenes where covers[ObjectIdentifier(scene)]?.windowScene !== scene {
            let window = UIWindow(windowScene: scene)
            window.windowLevel = .alert + 1
            window.rootViewController = UIHostingController(rootView: LockCover { Task { await self.unlock() } })
            window.isHidden = false
            covers[ObjectIdentifier(scene)] = window
        }
    }

    func unlock() async {
        guard !unlocking else { return }
        unlocking = true
        defer { unlocking = false }
        if await OwnerCheck.confirm("Unlock Herdr Remote") { locked = false }
    }
}

private struct LockCover: View {
    let unlock: () -> Void

    var body: some View {
        ZStack {
            Rectangle().fill(.regularMaterial).ignoresSafeArea()
            VStack(spacing: 14) {
                Image(systemName: "lock.fill").font(.largeTitle)
                Text("Herdr Remote is locked").font(.headline)
                Button("Unlock", action: unlock).buttonStyle(.glassProminent)
            }
        }
    }
}
