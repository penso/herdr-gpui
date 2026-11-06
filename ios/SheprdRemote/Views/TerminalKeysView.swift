import SheprdRemoteCore
import SwiftUI

/// Settings for the terminal's key rows: add, edit, and reorder their
/// keys, drag one to the other row, or set it aside in More Keys.
struct TerminalKeysView: View {
    @Environment(AppModel.self) private var model
    @State private var editing: Editing?

    private struct Editing: Identifiable {
        var key: TermKey
        var row: TermKeyLayout.Row
        let isNew: Bool
        var id: UUID { key.id }
    }

    var body: some View {
        let store = model.termKeys
        List {
            Section {
                Picker("Labels", selection: Bindable(store).symbols) {
                    Text("Words").tag(false)
                    Text("Symbols").tag(true)
                }
                .pickerStyle(.segmented)
            } footer: {
                Text("Symbols take less room: ⎋ ⌃ ⌥ ⇧ for esc, ctrl, alt, and shift. A row's keys share its width when they fit, and scroll when they do not, as in the preview below.")
            }
            // One list, not a section per row: a list drags only within
            // a section, and a key should move between the rows too.
            Section {
                ForEach(store.layout.entries) { entry in
                    if let key = entry.termKey {
                        keyLine(key)
                            .deleteDisabled(!store.layout.canDelete(key))
                    } else {
                        // Movable: List takes no drop right after a row it
                        // cannot move, and dragging a marker moves the edge
                        // it marks.
                        Text(entry == .bottomRow ? "Bottom Row" : "More Keys")
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .deleteDisabled(true)
                    }
                }
                .onDelete { store.layout.delete(atOffsets: $0) }
                .onMove { store.layout.move(fromOffsets: $0, toOffset: $1) }
            } header: {
                Text("Top Row")
            } footer: {
                Text("Drag keys between the rows and More Keys, or tap a key in More Keys to add it. Deleting a key from a row moves it to More Keys. The keyboard key always stays in a row.")
            }
            Section {
                Button("Restore Default Keys") { store.layout.restoreDefaults() }
                    .disabled(store.layout.isDefault)
            } footer: {
                Text("ctrl, alt, and shift always start the top row. Keys you made stay in More Keys.")
            }
        }
        .navigationTitle("Terminal Keys")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button("New Key", systemImage: "plus") { editing = Editing(key: TermKey(keys: []), row: .top, isNew: true) }
            }
            ToolbarItem(placement: .topBarTrailing) { EditButton() }
        }
        .sheet(item: $editing) { editing in
            KeyEditor(key: editing.key, row: editing.row, isNew: editing.isNew) { store.layout.put($0, in: $1) }
        }
        // Where the rows sit in a terminal, as wide, so what does not fit
        // scrolls there the same way.
        .safeAreaInset(edge: .bottom, spacing: 0) {
            KeyRowsPreview(layout: store.layout, symbols: store.symbols)
        }
    }

    @ViewBuilder
    private func keyLine(_ key: TermKey) -> some View {
        let layout = model.termKeys.layout
        if layout.row(of: key) == .spare {
            // A tap adds it: dragging from the end of a long list is slow.
            Menu {
                Button("Add to Top Row") { model.termKeys.layout.put(key, in: .top) }
                Button("Add to Bottom Row") { model.termKeys.layout.put(key, in: .bottom) }
                if !key.isStock {
                    Button("Edit…") { editing = Editing(key: key, row: .spare, isNew: false) }
                }
            } label: {
                face(key)
            }
        } else if key.special != nil {
            // Nothing to edit: it is only moved, or made spare.
            face(key)
        } else {
            Button {
                editing = Editing(key: key, row: layout.row(of: key) ?? .top, isNew: false)
            } label: {
                face(key)
            }
        }
    }

    private func face(_ key: TermKey) -> some View {
        LabeledContent {
            Text(key.spoken)
        } label: {
            if key.special == .dictation {
                Image(systemName: "mic.fill")
            } else {
                Text(key.label(symbols: model.termKeys.symbols)).font(.system(.body, design: .monospaced))
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// One key's row, label, and the keys it presses.
private struct KeyEditor: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State var key: TermKey
    @State var row: TermKeyLayout.Row
    let isNew: Bool
    let save: (TermKey, TermKeyLayout.Row) -> Void

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker("Row", selection: $row) {
                        Text("Top").tag(TermKeyLayout.Row.top)
                        Text("Bottom").tag(TermKeyLayout.Row.bottom)
                        Text("More Keys").tag(TermKeyLayout.Row.spare)
                    }
                    TextField("Label", text: $key.title, prompt: Text(key.keys.isEmpty ? "Shown on the key" : TermKey(keys: key.keys).label(symbols: model.termKeys.symbols)))
                }
                Section {
                    ForEach(Array(key.keys.enumerated()), id: \.offset) { _, name in
                        LabeledContent(TermKey(keys: [name]).spoken, value: TermKey(keys: [name]).label(symbols: model.termKeys.symbols))
                    }
                    .onDelete { key.keys.remove(atOffsets: $0) }
                    .onMove { key.keys.move(fromOffsets: $0, toOffset: $1) }
                    Menu("Add Key") {
                        ForEach(TermKey.choices, id: \.key) { choice in
                            Button("\(choice.symbol)  \(choice.name)") { key.keys.append(choice.key) }
                        }
                    }
                } header: {
                    Text("Presses")
                } footer: {
                    Text("In this order, so Tab then Enter accepts an agent's suggestion with one tap.")
                }
            }
            // The presses always show their handles; the other rows ignore it.
            .environment(\.editMode, .constant(.active))
            .navigationTitle(isNew ? "New Key" : "Edit Key")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save", role: .confirm) {
                        save(key, row)
                        dismiss()
                    }
                    .disabled(key.keys.isEmpty)
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}

/// The key rows as a terminal shows them, the fixed keys included.
private struct KeyRowsPreview: View {
    let layout: TermKeyLayout
    let symbols: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Preview").font(.caption).foregroundStyle(.secondary)
            TermKeyRow {
                ForEach(["ctrl", "alt", "shift"], id: \.self) { name in
                    TermKeyCap(label: TermKey.modifier(name, symbols: symbols), modifier: true)
                }
                ForEach(layout.top) { key in cap(key) }
            }
            TermKeyRow {
                ForEach(layout.bottom) { key in cap(key) }
            }
        }
        .padding(.horizontal, 8).padding(.vertical, 6)
        .background(.bar)
    }

    /// A key as `TermKeyRows` draws it.
    @ViewBuilder
    private func cap(_ key: TermKey) -> some View {
        switch key.special {
        case .dictation: Image(systemName: "mic.fill").font(.callout.weight(.medium)).termKeyFace()
        case .keyboard: TermKeyCap(label: "⌨︎")
        case nil: TermKeyCap(label: key.label(symbols: symbols), tint: key.keys == ["ctrl+c"] ? .red : nil)
        }
    }
}
