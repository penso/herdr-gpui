import Foundation

/// How Home lists the agents of every computer. Priority is the desktop's
/// agent panel order; workspace is its grouped one.
public enum AgentSort: String, CaseIterable, Identifiable, Sendable {
    case priority, recent, workspace

    public var id: Self { self }

    /// What a list needs to know about one of its rows.
    public struct Facts: Sendable {
        public var status: Home.AgentStatus
        /// When it last changed, if known.
        public var date: Date?
        public var title: String
        public var group: String
        public var groupTitle: String

        public init(status: Home.AgentStatus, date: Date?, title: String, group: String, groupTitle: String) {
            self.status = status
            self.date = date
            self.title = title
            self.group = group
            self.groupTitle = groupTitle
        }
    }

    public struct Section<Item>: Identifiable {
        /// Titles can repeat: two workspaces may share a name.
        public var id: String
        public var title: String
        public var items: [Item]
    }

    /// `items` in this order. Every section lists the newest first, then
    /// the undated by title.
    public func sections<Item>(of items: [Item], facts: (Item) -> Facts) -> [Section<Item>] {
        let rows = items.map { (item: $0, facts: facts($0)) }.sorted { a, b in
            switch (a.facts.date, b.facts.date) {
            case let (first?, second?) where first != second: first > second
            case (_?, nil): true
            case (nil, _?): false
            default: a.facts.title.localizedStandardCompare(b.facts.title) == .orderedAscending
            }
        }
        switch self {
        case .recent:
            return rows.isEmpty ? [] : [Section(id: "recent", title: "Recent", items: rows.map(\.item))]
        case .priority:
            let titled: [(Home.AgentStatus, String)] = [(.blocked, "Needs you"), (.done, "Done"), (.working, "Working"), (.idle, "Idle")]
            return titled.compactMap { status, title in
                let matching = rows.filter { ($0.facts.status == .unknown ? .idle : $0.facts.status) == status }
                return matching.isEmpty ? nil : Section(id: status.rawValue, title: title, items: matching.map(\.item))
            }
        case .workspace:
            // Groups keep the order of `items`, the computers' own.
            var order: [String] = []
            for item in items {
                let group = facts(item).group
                if !order.contains(group) { order.append(group) }
            }
            return order.compactMap { group in
                let matching = rows.filter { $0.facts.group == group }
                return matching.first.map { Section(id: group, title: $0.facts.groupTitle, items: matching.map(\.item)) }
            }
        }
    }
}
