import HerdrRemoteCore
import SwiftUI

/// Settings for the composer's text shortcuts: add, edit, reorder, and
/// delete them.
struct TextShortcutsView: View {
    @Environment(AppModel.self) private var model
    @State private var editing: TextShortcut?

    var body: some View {
        let store = model.shortcuts
        List {
            Section {
                ForEach(store.shortcuts) { shortcut in
                    Button {
                        editing = shortcut
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(shortcut.label).foregroundStyle(.primary)
                            Text(shortcut.text).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                        }
                    }
                }
                .onDelete { store.delete(at: $0) }
                .onMove { store.move(from: $0, to: $1) }
            } footer: {
                Text("Shortcuts appear under + in a chat. One sends at once when the message is empty, or is added to what you are writing.")
            }
        }
        .overlay {
            if store.shortcuts.isEmpty {
                ContentUnavailableView("No shortcuts", systemImage: "text.bubble",
                                       description: Text("Add text you send often, such as asking the agent to carry on."))
            }
        }
        .navigationTitle("Text Shortcuts")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button("Add", systemImage: "plus") { editing = TextShortcut(title: "", text: "") }
            }
            if !store.shortcuts.isEmpty {
                ToolbarItem(placement: .topBarTrailing) { EditButton() }
            }
        }
        .sheet(item: $editing) { shortcut in
            ShortcutEditor(shortcut: shortcut, isNew: !store.shortcuts.contains { $0.id == shortcut.id }) { store.save($0) }
        }
    }
}

/// One shortcut's title and text.
private struct ShortcutEditor: View {
    @Environment(\.dismiss) private var dismiss
    @State var shortcut: TextShortcut
    let isNew: Bool
    let save: (TextShortcut) -> Void

    var body: some View {
        NavigationStack {
            Form {
                Section("Title") {
                    TextField("Shown in the menu", text: $shortcut.title)
                }
                Section("Text") {
                    TextField("Sent to the agent", text: $shortcut.text, axis: .vertical)
                        .lineLimit(3...10)
                }
            }
            .navigationTitle(isNew ? "New Shortcut" : "Edit Shortcut")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save", role: .confirm) {
                        save(shortcut)
                        dismiss()
                    }
                    .disabled(!shortcut.isUsable)
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}
