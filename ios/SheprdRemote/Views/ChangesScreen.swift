import SheprdRemoteCore
import SwiftUI

/// What a screen is reading from the computer.
private enum Loading<Value> {
    case waiting
    case loaded(Value)
    case failed(String)
}

/// A workspace's whole change, read-only: its files for either base, and
/// its pull request's checks. Sheprd reads the checkout; this only shows
/// what it sends. The one thing it sends back is a prompt, after asking.
struct ChangesScreen: View {
    @Environment(AppModel.self) private var model
    let key: WorkspaceKey
    @State private var scope = ChangeScope.uncommitted
    @State private var files = Loading<ChangeList>.waiting
    @State private var checks: PullRequestChecks?
    @State private var sending: PullRequestCheck?
    #if DEBUG
    /// The simulator loop's `-openDiff PATH` opens that file's diff.
    @State private var debugFile: ChangedFile?
    #endif

    private var machine: Fleet.Machine? {
        model.fleet.machines.first { $0.id == key.device }
    }

    private var workspace: Home.Workspace? {
        machine?.home?.workspaces.first { $0.id == key.workspace }
    }

    /// What the lists are read for: another base, or a link that came back,
    /// as a phone waking from sleep still shows the address it had and the
    /// first try there fails.
    private struct Reading: Equatable {
        var scope: ChangeScope
        var status: LinkStatus?
    }

    var body: some View {
        List {
            Section {
                Picker("Compare", selection: $scope) {
                    Text("Uncommitted").tag(ChangeScope.uncommitted)
                    Text("Branch").tag(ChangeScope.branch)
                }
                .pickerStyle(.segmented)
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
            }
            if let checks, !checks.checks.isEmpty { checksSection(checks) }
            filesSection
        }
        .navigationTitle("Changes")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .refreshable { await reload() }
        .task(id: Reading(scope: scope, status: machine?.status)) { await loadFiles() }
        .task(id: machine?.status) { await loadChecks() }
        .sheet(item: $sending) { check in SendLogSheet(key: key, check: check) }
        #if DEBUG
        .navigationDestination(item: $debugFile) { FileDiffScreen(key: key, scope: scope, file: $0) }
        .onChange(of: debugFiles) { _, files in
            debugFile = files.first { $0.path == UserDefaults.standard.string(forKey: "openDiff") }
        }
        // `-sendLog YES` opens the confirmation for the first failing log.
        .onChange(of: checks) { _, checks in
            if UserDefaults.standard.bool(forKey: "sendLog") { sending = checks?.open.first { $0.job != nil } }
        }
        #endif
    }

    #if DEBUG
    private var debugFiles: [ChangedFile] {
        if case let .loaded(list) = files { list.files } else { [] }
    }
    #endif

    private func reload() async {
        async let files: Void = loadFiles()
        async let checks: Void = loadChecks()
        _ = await (files, checks)
    }

    private func loadFiles() async {
        let (device, id, scope) = (key.device, key.workspace, scope)
        files = .waiting
        do {
            let list = try await model.fleet.call(device) { client, token in
                try await client.changes(workspace: id, scope: scope, bearer: token)
            }
            guard !Task.isCancelled else { return }
            files = .loaded(list)
        } catch {
            guard !Task.isCancelled else { return }
            files = .failed(error.localizedDescription)
        }
    }

    /// Checks are extra here, so failing to read them only leaves them out.
    private func loadChecks() async {
        guard workspace?.pullRequest != nil else { return }
        let (device, id) = (key.device, key.workspace)
        checks = try? await model.fleet.call(device) { client, token in
            try await client.checks(workspace: id, bearer: token)
        }
    }

    @ViewBuilder private var filesSection: some View {
        switch files {
        case .waiting:
            Section { ProgressView().frame(maxWidth: .infinity) }
        case let .failed(message):
            Section {
                ContentUnavailableView("Cannot show the changes", systemImage: "exclamationmark.triangle", description: Text(message))
            }
        case let .loaded(list) where list.total == 0:
            Section {
                ContentUnavailableView(scope == .uncommitted ? "Nothing uncommitted" : "No changes on this branch",
                                       systemImage: "checkmark.circle")
            }
        case let .loaded(list):
            Section {
                ForEach(list.files) { file in
                    NavigationLink {
                        FileDiffScreen(key: key, scope: scope, file: file)
                    } label: {
                        FileRow(file: file)
                    }
                }
                if list.unlisted > 0 {
                    Text("\(list.unlisted) more files are too many to list here.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
            } header: {
                Text(header(list))
            }
        }
    }

    private func header(_ list: ChangeList) -> String {
        let files = list.total == 1 ? "1 file" : "\(list.total) files"
        return list.base.map { "\(files) against \($0)" } ?? files
    }

    private func checksSection(_ checks: PullRequestChecks) -> some View {
        Section {
            ForEach(checks.open) { check in CheckRow(check: check) { sending = check } }
            if !checks.settled.isEmpty {
                DisclosureGroup("\(checks.settled.count) passed or skipped") {
                    ForEach(checks.settled) { check in CheckRow(check: check) { sending = check } }
                }
            }
        } header: {
            Text(verbatim: checks.number.map { "Checks on #\($0)" } ?? "Checks")
        }
    }
}

/// A file's name, folder, and line counts.
private struct FileRow: View {
    var file: ChangedFile

    var body: some View {
        let name = (file.path as NSString).lastPathComponent
        let folder = (file.path as NSString).deletingLastPathComponent
        HStack(spacing: 10) {
            Image(systemName: symbol).foregroundStyle(color).accessibilityLabel(file.status.rawValue)
            VStack(alignment: .leading, spacing: 1) {
                Text(name).font(.body.monospaced()).lineLimit(1).truncationMode(.middle)
                if !folder.isEmpty {
                    Text(folder).font(.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.head)
                }
            }
            Spacer(minLength: 4)
            HStack(spacing: 6) {
                if let added = file.added, added > 0 { Text("+\(added)").foregroundStyle(.green) }
                if let removed = file.removed, removed > 0 { Text("−\(removed)").foregroundStyle(.red) }
            }
            .font(.caption.monospaced())
        }
    }

    private var symbol: String {
        switch file.status {
        case .added, .untracked: "plus.circle"
        case .deleted: "minus.circle"
        case .modified: "pencil.circle"
        case .renamed: "arrow.right.circle"
        }
    }

    private var color: Color {
        switch file.status {
        case .added, .untracked: .green
        case .deleted: .red
        case .modified, .renamed: .secondary
        }
    }
}

/// A check's state and name; a failure with a log offers to send it.
private struct CheckRow: View {
    var check: PullRequestCheck
    var send: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                Image(systemName: symbol).foregroundStyle(color).accessibilityLabel(label)
                Text(check.name).lineLimit(2).frame(maxWidth: .infinity, alignment: .leading)
                if let link = check.url.flatMap(URL.init(string:)) {
                    Link(destination: link) { Image(systemName: "arrow.up.right.square") }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("Open \(check.name)")
                }
            }
            if check.job != nil {
                Button("Send the log to the agent", systemImage: "text.bubble", action: send)
                    .buttonStyle(.borderless).font(.subheadline)
            }
        }
    }

    private var symbol: String {
        switch check.state {
        case .passed: "checkmark.circle.fill"
        case .failed: "xmark.circle.fill"
        case .pending: "clock"
        case .skipped: "minus.circle"
        }
    }

    private var color: Color {
        switch check.state {
        case .passed: .green
        case .failed: .red
        case .pending: .yellow
        case .skipped: .secondary
        }
    }

    private var label: String {
        switch check.state {
        case .passed: "Passed"
        case .failed: "Failed"
        case .pending: "Pending"
        case .skipped: "Skipped"
        }
    }
}

/// One file's unified diff. A long one is cut by the computer, and lines
/// are built as they scroll in, so neither stalls the app.
private struct FileDiffScreen: View {
    @Environment(AppModel.self) private var model
    let key: WorkspaceKey
    let scope: ChangeScope
    let file: ChangedFile
    @State private var diff = Loading<FileDiff>.waiting
    /// The screen's width, which colored lines fill however short.
    @State private var width: CGFloat?

    var body: some View {
        content
            .navigationTitle((file.path as NSString).lastPathComponent)
            .navigationBarTitleDisplayMode(.inline)
            .task { await load() }
    }

    @ViewBuilder private var content: some View {
        switch diff {
        case .waiting:
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        case let .failed(message):
            ContentUnavailableView("Cannot show this file", systemImage: "exclamationmark.triangle", description: Text(message))
        case let .loaded(diff) where diff.lines.isEmpty:
            ContentUnavailableView("Nothing to show", systemImage: "doc.text", description: Text(diff.note ?? ""))
        case let .loaded(diff):
            ScrollView([.vertical, .horizontal]) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    // First, so nobody reads to the end to learn there is more.
                    if let note = diff.note {
                        Label(note, systemImage: "scissors").font(.footnote).foregroundStyle(.secondary)
                            .padding(12).frame(width: width, alignment: .leading)
                    }
                    ForEach(Array(diff.lines.enumerated()), id: \.offset) { _, line in DiffLineRow(line: line) }
                }
                .frame(minWidth: width, alignment: .leading)
            }
            .onGeometryChange(for: CGFloat.self, of: \.size.width) { width = $0 }
            // Both axes would otherwise center a diff shorter than the screen.
            .defaultScrollAnchor(.topLeading)
        }
    }

    private func load() async {
        // A note needs no trip to the computer.
        if file.shown != .text {
            diff = .loaded(FileDiff(path: file.path, shown: file.shown))
            return
        }
        let (device, id, scope, path) = (key.device, key.workspace, scope, file.path)
        do {
            diff = .loaded(try await model.fleet.call(device) { client, token in
                try await client.fileDiff(path, workspace: id, scope: scope, bearer: token)
            })
        } catch {
            diff = .failed(error.localizedDescription)
        }
    }
}

private struct DiffLineRow: View {
    var line: DiffLine

    var body: some View {
        HStack(spacing: 0) {
            if line.kind != .hunk, line.kind != .meta {
                Text(number).foregroundStyle(.tertiary).frame(width: 44, alignment: .trailing).padding(.trailing, 8)
                    .accessibilityHidden(true)
            }
            Text(line.mark + line.text).foregroundStyle(line.kind == .hunk || line.kind == .meta ? .secondary : .primary)
                .lineLimit(1).fixedSize()
        }
        .font(.system(.caption, design: .monospaced))
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 8).padding(.vertical, 1)
        .background(background)
    }

    private var number: String {
        (line.new ?? line.old).map(String.init) ?? ""
    }

    private var background: Color {
        switch line.kind {
        case .added: .green.opacity(0.18)
        case .removed: .red.opacity(0.18)
        case .hunk: .secondary.opacity(0.12)
        case .context, .meta: .clear
        }
    }
}

/// The confirmation before a failing log goes to an agent: what will be
/// sent, to which chat. Nothing is sent until its button is pressed.
private struct SendLogSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: WorkspaceKey
    let check: PullRequestCheck
    @State private var excerpt = Loading<String>.waiting
    @State private var pane: String?
    @State private var sendingNow = false
    @State private var problem: String?

    private var chats: [ChatItem] {
        model.fleet.chatItems.filter { $0.computer.device == key.device && $0.workspace.id == key.workspace }
    }

    var body: some View {
        NavigationStack {
            Form {
                switch excerpt {
                case .waiting:
                    Section { ProgressView().frame(maxWidth: .infinity) }
                case let .failed(message):
                    Section { Text(message).foregroundStyle(.secondary) }
                case let .loaded(text):
                    if chats.isEmpty {
                        Section { Text("Start a chat in this workspace to send it this log.").foregroundStyle(.secondary) }
                    } else {
                        Section("Send to") {
                            Picker("Chat", selection: $pane) {
                                ForEach(chats) { item in Text(item.chat.title).tag(Optional(item.chat.pane)) }
                            }
                            .labelsHidden()
                            .pickerStyle(.inline)
                        }
                    }
                    Section("Log excerpt") {
                        Text(text).font(.caption.monospaced()).textSelection(.enabled)
                    }
                    if let problem { Section { Text(problem).foregroundStyle(.red) } }
                }
            }
            .navigationTitle(check.name)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Send") { send() }.disabled(!ready || sendingNow)
                }
            }
            .task { await load() }
        }
    }

    private var ready: Bool {
        if case .loaded = excerpt { pane != nil } else { false }
    }

    private func load() async {
        pane = pane ?? chats.first?.chat.pane
        guard let job = check.job else { return }
        let (device, id) = (key.device, key.workspace)
        do {
            excerpt = .loaded(try await model.fleet.call(device) { client, token in
                try await client.checkLog(job: job, workspace: id, bearer: token)
            }.excerpt)
        } catch {
            excerpt = .failed(error.localizedDescription)
        }
    }

    private func send() {
        guard case let .loaded(text) = excerpt, let pane else { return }
        let prompt = checkPrompt(check: check.name, excerpt: text)
        let device = key.device
        sendingNow = true
        problem = nil
        Task {
            defer { sendingNow = false }
            do {
                let rejected = try await model.fleet.call(device) { client, token in
                    try await client.command(.prompt(text: prompt), pane: pane, bearer: token)
                }
                if let rejected { problem = rejected } else { dismiss() }
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}
