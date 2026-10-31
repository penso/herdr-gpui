import SwiftUI

/// Shows one mockup, chosen by launch arguments:
/// `-screen home -variant A -state 0`. Without them, a list of all.
@main
struct MockupApp: App {
    var body: some Scene {
        WindowGroup {
            let defaults = UserDefaults.standard
            if let screen = defaults.string(forKey: "screen") {
                Mockup(screen: screen, variant: defaults.string(forKey: "variant") ?? "A",
                       state: defaults.integer(forKey: "state"))
            } else {
                Catalog()
            }
        }
    }
}

struct Mockup: View {
    var screen: String
    var variant: String
    var state: Int

    var body: some View {
        switch (screen, variant) {
        case ("pair", "A"): PairRadar(state: state)
        case ("pair", "B"): PairCard(state: state)
        case ("pair", "C"): PairSteps(state: state)
        case ("home", "A"): HomeGrouped()
        case ("home", "B"): HomeAttention()
        case ("home", "C"): HomeFiltered()
        case ("chat", "A"): ChatFocus()
        case ("chat", "B"): ChatDocument()
        case ("chat", "C"): ChatBubbles()
        case ("lock", "A"): LockQuestion()
        case ("lock", "B"): LockStepStack()
        case ("lock", "C"): IslandExpanded()
        case ("computers", "A"): ComputersGrouped()
        case ("computers", "B"): ComputersCarousel()
        case ("computers", "C"): ComputersTree()
        case ("new", "A"): NewWorkspaceForm()
        case ("new", "B"): NewWorkspacePicker()
        case ("new", "C"): NewWorkspacePrompt()
        case ("session", "A"): SessionList()
        case ("session", "B"): SessionPanel()
        case ("session", "C"): SessionCards()
        case ("term", "A"): TerminalFull()
        case ("term", "B"): TerminalPanes()
        case ("term", "C"): TerminalLine()
        case ("settings", "A"): SettingsGrouped()
        case ("settings", "B"): SettingsPerComputer()
        case ("settings", "C"): SettingsStatus()
        case ("ipad", "A"): IPadColumns()
        case ("ipad", "B"): IPadInspector()
        case ("ipad", "C"): IPadBoard()
        case ("connect", "Now"): ConnectionNow(state: state)
        case ("connect", "A"): ConnectionSubtitle(state: state)
        case ("connect", "B"): ConnectionCapsule(state: state)
        case ("connect", "C"): ConnectionToast(state: state)
        case ("connect", "D"): ConnectionQuiet(state: state)
        case ("sync", "A"): SyncEdge(state: state)
        case ("sync", "B"): SyncStrip(state: state)
        case ("sync", "C"): SyncTab(state: state)
        case ("sync", "D"): SyncIsland(state: state)
        case ("spaces", "A"): SpacesSidebar(state: state)
        case ("spaces", "B"): SpacesTerminal(state: state)
        case ("spaces", "C"): SpacesOneComputer(state: state)
        case ("spaces", "D"): SpacesFolded(state: state)
        case ("spaces", "Mix"): SpacesMix(state: state)
        default: Text("No mockup \(screen) \(variant)")
        }
    }
}

struct Catalog: View {
    private let entries: [(String, String, Int)] = [
        ("pair", "A", 0), ("pair", "A", 1), ("pair", "B", 0), ("pair", "B", 1), ("pair", "C", 0), ("pair", "C", 1),
        ("home", "A", 0), ("home", "B", 0), ("home", "C", 0),
        ("chat", "A", 0), ("chat", "B", 0), ("chat", "C", 0),
        ("lock", "A", 0), ("lock", "B", 0), ("lock", "C", 0),
        ("computers", "A", 0), ("computers", "B", 0), ("computers", "C", 0),
        ("new", "A", 0), ("new", "B", 0), ("new", "C", 0),
        ("session", "A", 0), ("session", "B", 0), ("session", "C", 0),
        ("term", "A", 0), ("term", "B", 0), ("term", "C", 0),
        ("settings", "A", 0), ("settings", "B", 0), ("settings", "C", 0),
        ("ipad", "A", 0), ("ipad", "B", 0), ("ipad", "C", 0),
        ("connect", "Now", 0), ("connect", "A", 0), ("connect", "B", 0), ("connect", "C", 0), ("connect", "D", 0),
        ("spaces", "A", 0), ("spaces", "A", 1), ("spaces", "B", 0), ("spaces", "B", 1),
        ("spaces", "C", 0), ("spaces", "C", 1), ("spaces", "D", 0), ("spaces", "D", 1), ("spaces", "Mix", 0), ("spaces", "Mix", 1),
    ]

    var body: some View {
        NavigationStack {
            List {
                ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                    NavigationLink("\(entry.0) \(entry.1) \(entry.2)") {
                        Mockup(screen: entry.0, variant: entry.1, state: entry.2)
                    }
                }
            }
            .navigationTitle("Mockups")
        }
    }
}
