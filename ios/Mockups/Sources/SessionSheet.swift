import SwiftUI

// The session sheet, opened from chat B's title. A: a plain settings list
// in a sheet. B: a glass panel dropping from the title, as the web app's
// sheet does. C: cards: usage, mode tiles, an effort slider.

private struct Mode: Identifiable {
    let id = UUID()
    var name: String
    var detail: String
    var symbol: String
}

private let modes: [Mode] = [
    Mode(name: "Default", detail: "Ask before edits and commands", symbol: "hand.raised"),
    Mode(name: "Accept edits", detail: "Edit files freely, ask before commands", symbol: "pencil"),
    Mode(name: "Plan", detail: "Read only until you approve a plan", symbol: "map"),
    Mode(name: "Bypass permissions", detail: "Run everything without asking", symbol: "bolt.fill"),
]

private struct UsageRow: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Context").font(.subheadline.weight(.medium))
                Spacer()
                Text("84k of 200k tokens · $0.84").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
            ProgressView(value: 0.42)
        }
    }
}

struct SessionList: View {
    var body: some View {
        NavigationStack { ChatDocumentContent() }
            .sheet(isPresented: .constant(true)) {
                NavigationStack {
                    List {
                        Section { UsageRow() }
                        Section("Model") {
                            ForEach(["Opus 5.5", "Sonnet 5.5", "Haiku 5.5"], id: \.self) { model in
                                HStack {
                                    Text(model)
                                    Spacer()
                                    if model == "Opus 5.5" { Image(systemName: "checkmark").foregroundStyle(.tint) }
                                }
                            }
                        }
                        Section("Effort") {
                            Picker("Effort", selection: .constant("High")) {
                                ForEach(["Low", "Medium", "High", "Max"], id: \.self) { Text($0) }
                            }
                            .pickerStyle(.segmented)
                        }
                        Section("Mode") {
                            ForEach(modes) { mode in
                                HStack {
                                    VStack(alignment: .leading, spacing: 1) {
                                        Text(mode.name)
                                        Text(mode.detail).font(.caption).foregroundStyle(.secondary)
                                    }
                                    Spacer()
                                    if mode.name == "Bypass permissions" { Image(systemName: "checkmark").foregroundStyle(.tint) }
                                }
                            }
                        }
                        Section {
                            Toggle("Extended thinking", isOn: .constant(true))
                            Toggle("Auto-accept questions", isOn: .constant(false))
                        } footer: {
                            Text("Auto-accept answers every question with its first option. Face ID confirms turning it on.")
                        }
                    }
                    .navigationTitle("Session")
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) { Button(role: .confirm) {} }
                    }
                }
                .presentationDetents([.fraction(0.8), .large])
            }
    }
}

struct SessionPanel: View {
    var body: some View {
        NavigationStack {
            ChatDocumentContent()
                .overlay {
                    Color.black.opacity(0.35).ignoresSafeArea()
                }
                .overlay(alignment: .top) {
                    VStack(alignment: .leading, spacing: 14) {
                        UsageRow()
                        Text("Model").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                        GlassEffectContainer {
                            HStack(spacing: 8) {
                                ForEach(["Opus 5.5", "Sonnet 5.5", "Haiku 5.5"], id: \.self) { model in
                                    Text(model).font(.subheadline.weight(.medium))
                                        .padding(.horizontal, 12).padding(.vertical, 8)
                                        .glassEffect(model == "Opus 5.5" ? .regular.tint(.accentColor).interactive() : .regular.interactive(), in: .capsule)
                                        .foregroundStyle(model == "Opus 5.5" ? .white : .primary)
                                }
                            }
                        }
                        Text("Effort").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                        Picker("Effort", selection: .constant("High")) {
                            ForEach(["Low", "Medium", "High", "Max"], id: \.self) { Text($0) }
                        }
                        .pickerStyle(.segmented)
                        Text("Mode").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                        Picker("Mode", selection: .constant("Bypass permissions")) {
                            ForEach(modes) { mode in Image(systemName: mode.symbol).tag(mode.name) }
                        }
                        .pickerStyle(.segmented)
                        Text("**Bypass permissions** · Run everything without asking").font(.footnote).foregroundStyle(.secondary)
                        Divider()
                        Toggle("Extended thinking", isOn: .constant(true))
                        Toggle("Auto-accept questions", isOn: .constant(false))
                    }
                    .padding(18)
                    .glassEffect(.regular, in: .rect(cornerRadius: 30))
                    .padding(.horizontal, 10)
                }
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .principal) {
                        HStack(spacing: 4) {
                            Text("Equalize splits command").font(.headline)
                            Image(systemName: "chevron.up").font(.caption2.weight(.bold)).foregroundStyle(.secondary)
                        }
                    }
                }
        }
    }
}

struct SessionCards: View {
    var body: some View {
        NavigationStack { ChatDocumentContent() }
            .sheet(isPresented: .constant(true)) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        HStack(spacing: 16) {
                            ZStack {
                                Circle().stroke(.quaternary, lineWidth: 8)
                                Circle().trim(from: 0, to: 0.42).stroke(.tint, style: StrokeStyle(lineWidth: 8, lineCap: .round))
                                    .rotationEffect(.degrees(-90))
                                Text("42%").font(.headline.monospacedDigit())
                            }
                            .frame(width: 76, height: 76)
                            VStack(alignment: .leading, spacing: 3) {
                                Text("Opus 5.5").font(.title3.bold())
                                Text("84k of 200k tokens").font(.subheadline).foregroundStyle(.secondary)
                                Text("$0.84 this chat · 62 tok/s").font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer()
                            Image(systemName: "chevron.up.chevron.down").foregroundStyle(.secondary)
                        }
                        Text("Mode").font(.headline).padding(.top, 6)
                        LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
                            ForEach(modes) { mode in
                                let selected = mode.name == "Bypass permissions"
                                VStack(alignment: .leading, spacing: 6) {
                                    Image(systemName: mode.symbol).font(.title3)
                                    Text(mode.name).font(.subheadline.weight(.semibold))
                                    Text(mode.detail).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                                }
                                .frame(maxWidth: .infinity, minHeight: 100, alignment: .topLeading)
                                .padding(12)
                                .background(selected ? AnyShapeStyle(.tint.opacity(0.15)) : AnyShapeStyle(.fill.quinary), in: .rect(cornerRadius: 18))
                                .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), lineWidth: 2))
                            }
                        }
                        Text("Effort").font(.headline).padding(.top, 6)
                        Slider(value: .constant(0.67), in: 0...1, step: 0.33)
                        HStack {
                            ForEach(["Low", "Medium", "High", "Max"], id: \.self) { level in
                                Text(level).font(.caption).foregroundStyle(level == "High" ? .primary : .secondary)
                                if level != "Max" { Spacer() }
                            }
                        }
                        Toggle(isOn: .constant(false)) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Auto-accept questions").font(.subheadline.weight(.semibold))
                                Text("Answers with the first option. Needs Face ID.").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        .padding(14)
                        .background(.red.opacity(0.1), in: .rect(cornerRadius: 18))
                        .padding(.top, 6)
                    }
                    .padding(20)
                }
                .presentationDetents([.fraction(0.85)])
            }
    }
}
