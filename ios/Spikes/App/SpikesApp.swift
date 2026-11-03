import ActivityKit
import HerdrRemoteCore
import SwiftUI
import UIKit
import UserNotifications

/// Three spikes on three tabs; each shows what it measured so a device run
/// can be read off the screen.
@main
struct SpikesApp: App {
    @UIApplicationDelegateAdaptor private var delegate: SpikeDelegate
    @State private var central = Central()
    /// `-tab notify` or `-tab activity` opens that tab, for headless runs.
    @State private var tab = UserDefaults.standard.string(forKey: "tab") ?? "bluetooth"

    var body: some Scene {
        WindowGroup {
            TabView(selection: $tab) {
                Tab("Bluetooth", systemImage: "dot.radiowaves.left.and.right", value: "bluetooth") { BluetoothTab(central: central) }
                Tab("Notify", systemImage: "bell.badge", value: "notify") { NotifyTab() }
                Tab("Activity", systemImage: "rectangle.badge.checkmark", value: "activity") { ActivityTab() }
            }
            .task { central.start() }
        }
    }
}

final class SpikeDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        UserDefaults(suiteName: SpikeIDs.appGroup)?.set(deviceToken.map { String(format: "%02x", $0) }.joined(), forKey: "apns-token")
    }
}

struct BluetoothTab: View {
    let central: Central

    var body: some View {
        NavigationStack {
            List(central.lines.reversed(), id: \.self) { Text($0).font(.caption.monospaced()) }
                .navigationTitle("Bluetooth")
                .toolbar { Button("Clear") { central.clear() } }
        }
    }
}

struct NotifyTab: View {
    @State private var publicKey = ""
    @State private var token = ""
    @State private var problem: String?
    @State private var delivered: [String] = []

    var body: some View {
        NavigationStack {
            Form {
                Section("Key-agreement key (seal to this)") {
                    Text(publicKey.isEmpty ? "none" : publicKey).font(.caption.monospaced()).textSelection(.enabled)
                    Button("Make a new key") { make() }
                }
                Section("APNs token") {
                    Text(token.isEmpty ? "not registered" : token).font(.caption.monospaced()).textSelection(.enabled)
                    Button("Register") {
                        Task {
                            _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound])
                            UIApplication.shared.registerForRemoteNotifications()
                        }
                    }
                }
                Section("Delivered (as the extension left them)") {
                    ForEach(delivered, id: \.self) { Text($0).font(.caption) }
                }
                if let problem { Section("Problem") { Text(problem) } }
            }
            .navigationTitle("Sealed notifications")
            .task {
                // Provisional: granted without a prompt, delivered quietly;
                // the service extension still runs.
                _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .provisional])
                await refresh()
            }
            .refreshable { await refresh() }
        }
    }

    private func make() {
        do {
            let key = try NotifyKey.create()
            share(key)
        } catch {
            problem = "\(error)"
        }
    }

    private func refresh() async {
        do {
            share(try NotifyKey.load() ?? NotifyKey.create())
        } catch {
            problem = "\(error)"
        }
        token = UserDefaults(suiteName: SpikeIDs.appGroup)?.string(forKey: "apns-token") ?? ""
        let notes = await UNUserNotificationCenter.current().deliveredNotifications()
        delivered = notes.map { "\($0.request.content.title) — \($0.request.content.body)" }
        write(delivered.joined(separator: "\n"), to: "delivered.txt")
    }

    private func write(_ text: String, to name: String) {
        let folder = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: SpikeIDs.appGroup)
        guard let file = folder?.appending(path: name) else { return }
        try? Data(text.utf8).write(to: file)
    }

    /// Shows the public key and writes it to the App Group, where a test
    /// script reads it from the simulator's container.
    private func share(_ key: NotifyKey.Stored) {
        publicKey = Base64URL.encode(key.publicKey.x963Representation)
        write(publicKey, to: "notify-public-key.txt")
    }
}

struct ActivityTab: View {
    @State private var link = SpikeTarget.link ?? ""
    @State private var result = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("Pairing link (address and CA to call)") {
                    TextField("https://…/pair#t=…&fp=…", text: $link, axis: .vertical)
                        .font(.caption.monospaced())
                    Button("Save") { SpikeTarget.link = link }
                }
                Section {
                    Button("Start the activity") { start() }
                    Button("Run the intent here") {
                        Task { result = await SpikeAnswerIntent.call() }
                    }
                    if !result.isEmpty { Text(result).font(.caption.monospaced()) }
                } footer: {
                    Text("Then lock the phone and press Allow once on the lock screen.")
                }
            }
            .navigationTitle("Live Activity")
            .task {
                // `-startActivity YES` starts one at launch, for headless runs.
                if UserDefaults.standard.bool(forKey: "startActivity") { start() }
            }
        }
    }

    private func start() {
        do {
            _ = try Activity.request(
                attributes: SpikeActivityAttributes(chat: "herdr-gpui · Fix mise discovery"),
                content: .init(state: .init(question: "Run git push origin fix/mise?", result: nil), staleDate: nil)
            )
            result = "started"
        } catch {
            result = "\(error)"
        }
    }
}
