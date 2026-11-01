import HerdrRemoteCore
import LocalAuthentication
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// Sends what was shared (text, a link, up to four pictures) to a chat as
/// a prompt. It signs in as this phone with the keys the app keeps in the
/// App Group, and lists chats from the app's last summary.
final class ShareViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        let model = ShareModel(items: extensionContext?.inputItems as? [NSExtensionItem] ?? []) { [weak self] sent in
            if sent {
                self?.extensionContext?.completeRequest(returningItems: nil)
            } else {
                self?.extensionContext?.cancelRequest(withError: CocoaError(.userCancelled))
            }
        }
        let host = UIHostingController(rootView: ShareView(model: model))
        addChild(host)
        host.view.frame = view.bounds
        host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(host.view)
        host.didMove(toParent: self)
        Task {
            guard await model.unlock() else { return }
            await model.load()
        }
    }
}

@MainActor
@Observable
final class ShareModel {
    var text = ""
    var pictures: [UIImage] = []
    var chat: WidgetSummary.Chat?
    /// Empty until the owner is confirmed: chat titles, computers, and
    /// workspaces are what the app lock hides.
    private(set) var chats: [WidgetSummary.Chat] = []
    private(set) var unlocked = false
    var problem: String?
    var sending = false
    private let items: [NSExtensionItem]
    private let finish: (Bool) -> Void

    init(items: [NSExtensionItem], finish: @escaping (Bool) -> Void) {
        self.items = items
        self.finish = finish
    }

    /// Asks for Face ID or the passcode when the app is locked (Settings >
    /// Privacy), before anything about the chats shows; then lists them.
    /// A refusal closes the sheet.
    func unlock() async -> Bool {
        if UserDefaults(suiteName: "group.so.pen.herdr.remote")?.bool(forKey: "requireFaceID") == true,
           !((try? await LAContext().evaluatePolicy(.deviceOwnerAuthentication, localizedReason: "Send to Herdr")) ?? false) {
            finish(false)
            return false
        }
        chats = WidgetSummary.load().ranked
        chat = chats.first
        unlocked = true
        return true
    }

    /// Reads the shared text, link, and pictures.
    func load() async {
        var parts: [String] = []
        for provider in items.flatMap({ $0.attachments ?? [] }) {
            if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier),
               let url = try? await provider.loadItem(forTypeIdentifier: UTType.url.identifier) as? URL {
                parts.append(url.absoluteString)
            } else if provider.hasItemConformingToTypeIdentifier(UTType.image.identifier), pictures.count < 4,
                      let item = try? await provider.loadItem(forTypeIdentifier: UTType.image.identifier),
                      let image = Self.image(item) {
                pictures.append(image)
            } else if provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier),
                      let text = try? await provider.loadItem(forTypeIdentifier: UTType.plainText.identifier) as? String {
                parts.append(text)
            }
        }
        text = parts.joined(separator: "\n")
    }

    /// A shared picture arrives as data, a file, or an image, by app.
    private static func image(_ item: NSSecureCoding) -> UIImage? {
        switch item {
        case let image as UIImage: image
        case let data as Data: UIImage(data: data)
        case let url as URL: (try? Data(contentsOf: url)).flatMap(UIImage.init(data:))
        default: nil
        }
    }

    func send() {
        guard unlocked, let chat else { return }
        let prompt = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let jpegs = pictures.compactMap { PreparedImage($0)?.jpeg }
        guard !prompt.isEmpty || !jpegs.isEmpty else { return }
        sending = true
        Task {
            do {
                try await BackgroundCommands.sendPrompt(prompt, pictures: jpegs, computer: chat.computerId, pane: chat.pane)
                finish(true)
            } catch {
                problem = error.localizedDescription
                sending = false
            }
        }
    }

    func cancel() { finish(false) }
}

struct ShareView: View {
    @Bindable var model: ShareModel

    var body: some View {
        NavigationStack {
            Form {
                if !model.unlocked {
                    Section { ProgressView().frame(maxWidth: .infinity) }
                } else if model.chats.isEmpty {
                    Section {
                        Text("Open Herdr Remote once with a computer connected, so its chats are listed here.")
                            .foregroundStyle(.secondary)
                    }
                } else {
                    Section("Chat") {
                        Picker("Chat", selection: $model.chat) {
                            ForEach(model.chats) { chat in
                                VStack(alignment: .leading) {
                                    Text(chat.title)
                                    Text("\(chat.computer) · \(chat.workspace)").font(.caption).foregroundStyle(.secondary)
                                }
                                .tag(Optional(chat))
                            }
                        }
                        .pickerStyle(.navigationLink)
                    }
                }
                Section("Prompt") {
                    TextField("What should it do with this?", text: $model.text, axis: .vertical).lineLimit(3...10)
                }
                if !model.pictures.isEmpty {
                    Section("Pictures") {
                        ScrollView(.horizontal) {
                            HStack {
                                ForEach(Array(model.pictures.enumerated()), id: \.offset) { _, picture in
                                    Image(uiImage: picture).resizable().scaledToFill().frame(width: 64, height: 64).clipShape(.rect(cornerRadius: 10))
                                }
                            }
                        }
                    }
                }
                if let problem = model.problem {
                    Section { Text(problem).foregroundStyle(.red) }
                }
            }
            .navigationTitle("Send to Herdr")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { model.cancel() } }
                ToolbarItem(placement: .confirmationAction) {
                    if model.sending { ProgressView() } else {
                        Button("Send") { model.send() }.disabled(model.chat == nil)
                    }
                }
            }
        }
    }
}
