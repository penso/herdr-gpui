import Foundation
import HerdrRemoteCore

extension MessageIndex {
    /// The app's index, beside the cache in the app group.
    static let shared = MessageIndex(url: Cache.base.appending(path: "Index/Messages.sqlite"))
}

/// A message that matched a search, in a chat the lists show.
struct MessageHit: Identifiable, Hashable {
    let item: ChatItem
    let hit: SearchHit
    /// From this phone's index, as the computer could not search.
    let local: Bool

    var id: String { "\(item.id)/\(hit.entry.value)" }
}

extension Fleet {
    /// Messages matching `text` across computers, in their order: each
    /// online computer searches its own chats, and this phone's index
    /// answers for the rest, offline or running a daemon without search.
    /// Waits a moment first, so typing cancels it before it asks.
    func searchMessages(_ text: String) async -> [MessageHit] {
        guard let query = MessageText.query(text) else { return [] }
        try? await Task.sleep(for: .milliseconds(250))
        guard !Task.isCancelled else { return [] }
        let online = machines.compactMap { machine -> String? in
            if case .online = machine.status { machine.id } else { nil }
        }
        var answers: [String: [SearchHit]] = [:]
        await withTaskGroup(of: (String, [SearchHit]?).self) { group in
            for device in online {
                group.addTask { (device, await self.search(query, on: device)) }
            }
            for await (device, hits) in group {
                if let hits { answers[device] = hits }
            }
        }
        let unsearched = Set(machines.map(\.id)).subtracting(answers.keys)
        for match in (try? await MessageIndex.shared.search(query, devices: unsearched)) ?? [] {
            answers[match.device, default: []].append(match.hit)
        }
        let items = Dictionary(chatItems.map { ($0.key, $0) }, uniquingKeysWith: { first, _ in first })
        // A chat no list shows any more cannot be opened.
        return machines.flatMap { machine in
            (answers[machine.id] ?? []).compactMap { hit in
                items[PaneKey(device: machine.id, pane: hit.pane)].map {
                    MessageHit(item: $0, hit: hit, local: unsearched.contains(machine.id))
                }
            }
        }
    }

    /// What `device` found; nil when it cannot search or did not answer.
    private func search(_ query: String, on device: String) async -> [SearchHit]? {
        do {
            return try await call(device) { client, token in
                try await client.search(query, bearer: token)
            }?.hits
        } catch {
            return nil
        }
    }
}
