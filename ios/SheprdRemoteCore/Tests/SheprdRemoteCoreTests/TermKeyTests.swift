import Foundation
import Testing
@testable import SheprdRemoteCore

@Suite struct TermKeyTests {
    @Test func theDefaultRowsOfferEnterBesideTab() throws {
        let top = TermKeyLayout.load(nil).top.map(\.keys)
        let tab = try #require(top.firstIndex(of: ["tab"]))
        // Accepting an agent's suggestion is Tab, then Enter.
        #expect(top[tab + 1] == ["enter"])
        #expect(TermKeyLayout.load(nil) == .defaults)
    }

    @Test func enterAndANewLineLookAndSoundDifferent() {
        #expect(TermKey(keys: ["enter"]).label() == "⏎")
        #expect(TermKey(keys: ["ctrl+j"]).label() == "⇧⏎")
        #expect(TermKey(keys: ["enter"]).spoken == "Enter")
        #expect(TermKey(keys: ["ctrl+j"]).spoken == "New line")
    }

    @Test func aKeyCanPressSeveralKeysInOrder() {
        let accept = TermKey(keys: ["tab", "enter"])
        #expect(accept.label() == "⇥⏎")
        #expect(accept.spoken == "Tab, Enter")
        let named = TermKey(title: " OK ", keys: ["tab", "enter"])
        #expect(named.label() == "OK")
        #expect(named.spoken == "OK")
    }

    @Test func keysWrittenAsWordsCanShowSymbolsInstead() {
        #expect(TermKey(keys: ["esc"]).label() == "esc")
        #expect(TermKey(keys: ["esc"]).label(symbols: true) == "⎋")
        #expect(TermKey(keys: ["esc", "enter"]).label(symbols: true) == "⎋⏎")
        // Keys that already show a symbol keep it either way.
        #expect(TermKey(keys: ["tab"]).label() == "⇥")
        #expect(TermKey(keys: ["tab"]).label(symbols: true) == "⇥")
        #expect(TermKey(keys: ["ctrl+d"]).label(symbols: true) == "^D")
        // A title wins.
        #expect(TermKey(title: "Go", keys: ["esc"]).label(symbols: true) == "Go")
        // The fixed modifiers too.
        #expect(["ctrl", "alt", "shift"].map { TermKey.modifier($0, symbols: false) } == ["ctrl", "alt", "shift"])
        #expect(["ctrl", "alt", "shift"].map { TermKey.modifier($0, symbols: true) } == ["⌃", "⌥", "⇧"])
    }

    @Test func onlyASingleKeyOtherThanEnterRepeatsWhileHeld() {
        #expect(TermKey(keys: ["up"]).repeats)
        #expect(TermKey(keys: ["backspace"]).repeats)
        // A long press must not answer an agent twice.
        #expect(!TermKey(keys: ["enter"]).repeats)
        #expect(!TermKey(keys: ["tab", "enter"]).repeats)
        // Legacy input sends these as the same carriage return.
        #expect(!TermKey(keys: ["ctrl+enter"]).repeats)
        #expect(!TermKey(keys: ["shift+enter"]).repeats)
        #expect(TermKey(keys: ["ctrl+left"]).repeats)
    }

    @Test func savedRowsRoundTripWithTheirSpareKeys() throws {
        var layout = TermKeyLayout.defaults
        layout.put(TermKey(keys: ["tab", "enter"]), in: .bottom)
        let tilde = try #require(layout.entries.firstIndex { $0.termKey?.keys == ["~"] })
        layout.delete(atOffsets: [tilde])
        #expect(layout.spare.first?.keys == ["~"])
        #expect(TermKeyLayout.load(try TermKeyLayout.save(layout)) == layout)
        // Loading what was loaded changes nothing.
        let once = TermKeyLayout.load(try TermKeyLayout.save(layout))
        #expect(TermKeyLayout.load(try TermKeyLayout.save(once)) == once)
    }

    @Test func everyKeyIsInARowOrSpareExactlyOnce() {
        let defaults = TermKeyLayout.defaults
        let names = (defaults.top + defaults.bottom + defaults.spare).map { $0.keys.joined(separator: " ") }
        #expect(Set(names).count == names.count)
        #expect(Set(names) == Set(TermKey.choices.map(\.key) + TermKey.Special.allCases.map(\.rawValue)))
        #expect(defaults.spare.contains { $0.keys == ["shift+tab"] })
        #expect(!defaults.spare.contains { $0.special == .keyboard })
        #expect(defaults.spare.allSatisfy { $0.isStock })
    }

    @Test func controlCDictationAndTheKeyboardAreKeysThatMoveLikeAnyOther() {
        let bottom = TermKeyLayout.defaults.bottom
        #expect(bottom.first?.keys == ["ctrl+c"])
        #expect(bottom.dropFirst().first?.special == .dictation)
        #expect(bottom.last?.special == .keyboard)
        #expect(TermKey(keys: ["ctrl+c"]).label() == "^C")
        #expect(TermKey(special: .keyboard).spoken == "Keyboard")
        #expect(TermKey(special: .dictation).spoken == "Dictation")
        #expect(TermKey(keys: ["esc"]).special == nil)
        #expect(!TermKey(special: .keyboard).repeats)
        // Deleting from a row only makes a key spare; the keyboard key stays,
        // as the keyboard could not come back without it.
        let esc = TermKey(keys: ["esc"])
        var layout = TermKeyLayout(top: [TermKey(special: .keyboard)], bottom: [esc])
        layout.delete(atOffsets: [0, 2])
        #expect(layout.top.map(\.special) == [.keyboard])
        #expect(layout.bottom.isEmpty)
        #expect(layout.spare == [esc])
    }

    @Test func spareKeysAreDeletedOnlyWhenThePersonMadeThem() {
        let esc = TermKey(keys: ["esc"])
        let accept = TermKey(keys: ["tab", "enter"])
        let quit = TermKey(title: "Quit", keys: ["ctrl+c"])
        var layout = TermKeyLayout(top: [], bottom: [TermKey(special: .keyboard)], spare: [esc, accept, quit])
        #expect(esc.isStock && !accept.isStock && !quit.isStock)
        // [Bottom Row, keyboard, More Keys, esc, accept, quit]
        layout.delete(atOffsets: [3, 4, 5])
        #expect(layout.spare == [esc])
    }

    @Test func aSpareCopyOfAStockKeyGoesButTheLastOneStays() {
        let esc = TermKey(keys: ["esc"])
        let copy = TermKey(keys: ["esc"])
        let again = TermKey(keys: ["esc"])
        let keyboard = TermKey(special: .keyboard)
        var layout = TermKeyLayout(top: [], bottom: [keyboard], spare: [esc, copy, again])
        #expect(layout.canDelete(copy) && !layout.canDelete(keyboard))
        // [Bottom Row, keyboard, More Keys, esc, copy, again], all at once.
        layout.delete(atOffsets: [3, 4, 5])
        #expect(layout.spare == [again])
        #expect(!layout.canDelete(again))
        // One on a row lets the spare one go.
        var rows = TermKeyLayout(top: [esc], bottom: [keyboard], spare: [copy])
        // [esc, Bottom Row, keyboard, More Keys, copy]
        rows.delete(atOffsets: [4])
        #expect(rows.top == [esc] && rows.spare.isEmpty)
    }

    @Test func keysALaterBuildOffersJoinMoreKeys() throws {
        // Saved by a build that offered only these.
        let saved = TermKeyLayout(top: [TermKey(keys: ["esc"])], bottom: [TermKey(special: .keyboard)])
        let loaded = TermKeyLayout.load(try TermKeyLayout.save(saved))
        #expect(loaded.top == saved.top && loaded.bottom == saved.bottom)
        #expect(loaded.spare.contains { $0.keys == ["tab"] } && loaded.spare.contains { $0.special == .dictation })
    }

    @Test func theDefaultsAreTheDefaultsInAnyLaunch() {
        // Another launch makes the same keys with new ids.
        func fresh(_ keys: [TermKey]) -> [TermKey] { keys.map { TermKey(title: $0.title, keys: $0.keys) } }
        let defaults = TermKeyLayout.defaults
        #expect(TermKeyLayout(top: fresh(defaults.top), bottom: fresh(defaults.bottom), spare: defaults.spare).isDefault)
        var moved = TermKeyLayout(top: fresh(defaults.top), bottom: fresh(defaults.bottom))
        moved.top.swapAt(0, 1)
        #expect(!moved.isDefault)
    }

    @Test func unreadableDataFallsBackToTheDefaults() {
        #expect(TermKeyLayout.load(Data("not json".utf8)) == .defaults)
    }

    @Test func aKeyIsReplacedInPlaceOrMovedToTheEndOfItsNewPlace() {
        let first = TermKey(keys: ["esc"])
        let second = TermKey(keys: ["tab"])
        var layout = TermKeyLayout(top: [first, second], bottom: [])
        var renamed = first
        renamed.title = "Esc"
        layout.put(renamed, in: .top)
        #expect(layout.top == [renamed, second])
        layout.put(renamed, in: .bottom)
        #expect(layout.top == [second])
        #expect(layout.bottom == [renamed])
        #expect(layout.row(of: renamed) == .bottom)
        let added = TermKey(keys: ["enter"])
        layout.put(added, in: .bottom)
        #expect(layout.bottom == [renamed, added])
        #expect(layout.row(of: TermKey(keys: ["up"])) == nil)
        // A spare key put in a row is no longer spare.
        let spare = TermKey(keys: ["backspace"])
        layout.spare = [spare]
        #expect(layout.row(of: spare) == .spare)
        layout.put(spare, in: .top)
        #expect(!layout.spare.contains(spare))
        #expect(layout.top.last == spare)
    }

    @Test func aKeyDraggedPastAMarkerChangesRowsOrBecomesSpare() {
        let esc = TermKey(keys: ["esc"])
        let tab = TermKey(keys: ["tab"])
        let up = TermKey(keys: ["up"])
        let delete = TermKey(keys: ["backspace"])
        var layout = TermKeyLayout(top: [esc, tab], bottom: [up], spare: [delete])
        // One list, as the editor shows it.
        #expect(layout.entries == [.key(esc), .key(tab), .bottomRow, .key(up), .spareKeys, .key(delete)])
        // Tab dragged below up, above More Keys.
        layout.move(fromOffsets: [1], toOffset: 4)
        #expect(layout == TermKeyLayout(top: [esc], bottom: [up, tab], spare: [delete]))
        // Up dragged to the very top.
        layout.move(fromOffsets: [2], toOffset: 0)
        #expect(layout == TermKeyLayout(top: [up, esc], bottom: [tab], spare: [delete]))
        // Within a row, as before.
        layout.move(fromOffsets: [1], toOffset: 0)
        #expect(layout == TermKeyLayout(top: [esc, up], bottom: [tab], spare: [delete]))
        // A row can be emptied, and filled again from the other.
        layout.move(fromOffsets: [3], toOffset: 2)
        #expect(layout == TermKeyLayout(top: [esc, up, tab], bottom: [], spare: [delete]))
        layout.move(fromOffsets: [0], toOffset: 4)
        #expect(layout == TermKeyLayout(top: [up, tab], bottom: [esc], spare: [delete]))
        // A spare key dragged into a row, and a row key dragged below More Keys.
        layout.move(fromOffsets: [5], toOffset: 0)
        #expect(layout == TermKeyLayout(top: [delete, up, tab], bottom: [esc], spare: []))
        layout.move(fromOffsets: [1], toOffset: 6)
        #expect(layout == TermKeyLayout(top: [delete, tab], bottom: [esc], spare: [up]))
        // Deleting from the rows makes keys spare, first in More Keys.
        layout.delete(atOffsets: [0, 3])
        #expect(layout == TermKeyLayout(top: [tab], bottom: [], spare: [delete, esc, up]))
    }

    @Test func draggingAMarkerMovesTheBoundaryItMarks() {
        let esc = TermKey(keys: ["esc"])
        let tab = TermKey(keys: ["tab"])
        let up = TermKey(keys: ["up"])
        var layout = TermKeyLayout(top: [esc, tab], bottom: [up], spare: [])
        // [esc, tab, Bottom Row, up, More Keys]: Bottom Row up one.
        layout.move(fromOffsets: [2], toOffset: 1)
        #expect(layout == TermKeyLayout(top: [esc], bottom: [tab, up], spare: []))
        // More Keys up one: up becomes spare.
        layout.move(fromOffsets: [4], toOffset: 3)
        #expect(layout == TermKeyLayout(top: [esc], bottom: [tab], spare: [up]))
        // The markers cannot trade places.
        let before = layout
        layout.move(fromOffsets: [1], toOffset: 4)
        #expect(layout == before)
    }

    @Test func theKeyboardKeyCannotBeDraggedToTheSpareKeys() {
        let keyboard = TermKey(special: .keyboard)
        let esc = TermKey(keys: ["esc"])
        let layout = TermKeyLayout(top: [], bottom: [keyboard], spare: [esc])
        var moved = layout
        // [Bottom Row, keyboard, More Keys, esc]
        moved.move(fromOffsets: [1], toOffset: 4)
        #expect(moved == layout)
        // Into the top row is fine.
        moved.move(fromOffsets: [1], toOffset: 0)
        #expect(moved.top == [keyboard])
    }

    @Test func restoringTheDefaultsKeepsKeysThePersonMade() {
        var layout = TermKeyLayout.defaults
        let accept = TermKey(keys: ["tab", "enter"])
        layout.put(accept, in: .top)
        #expect(!layout.isDefault)
        layout.restoreDefaults()
        #expect(layout.isDefault)
        #expect(layout.spare.first == accept)
    }

    @Test func restoringAfterRenamingADefaultKeyKeepsEveryKeyApart() {
        var layout = TermKeyLayout.defaults
        var quit = layout.top[0]
        quit.title = "Quit"
        layout.put(quit, in: .top)
        layout.restoreDefaults()
        let ids = layout.entries.map(\.id)
        #expect(Set(ids).count == ids.count)
        #expect(layout.isDefault && layout.spare.first == quit && layout.row(of: quit) == .spare)
    }

    @Test func editingTheOnlyCopyOfAStockKeyStillOffersIt() {
        var layout = TermKeyLayout.defaults
        var accept = layout.top[0]
        #expect(accept.keys == ["esc"] && !layout.spare.contains { $0.keys == ["esc"] })
        accept.keys = ["tab", "enter"]
        layout.put(accept, in: .top)
        #expect(layout.top[0] == accept)
        #expect(layout.spare.contains { $0.isStock && $0.keys == ["esc"] })
    }

    @Test func linesHaveDistinctIdentities() {
        let ids = TermKeyLayout.defaults.entries.map(\.id)
        #expect(Set(ids).count == ids.count)
    }

    /// Rows as saved before spare keys existed.
    @Test func aCombinationIsModifiersAndOneKey() {
        #expect(TermKey.combination([.ctrl], "b") == "ctrl+b")
        #expect(TermKey.combination([.ctrl], "B") == "ctrl+b")
        #expect(TermKey.combination([.alt], "x") == "alt+x")
        #expect(TermKey.combination([.alt, .ctrl], "F5") == "ctrl+alt+f5")
        #expect(TermKey.combination([.alt], "+") == "alt+plus")
        #expect(TermKey.combination([.ctrl], "space") == "ctrl+space")
        #expect(TermKey.combination([.shift], "tab") == "shift+tab")
        #expect(TermKey.combination([], "q") == "q")
        // Spaces around what was typed are not part of it.
        #expect(TermKey.combination([.alt], " b ") == "alt+b")
        #expect(TermKey.combination([.ctrl], " f5") == "ctrl+f5")
        // Not one character to the daemon: several scalars (👍🏻, a flag,
        // İ lowercased by ctrl into i and a dot), or a control (U+0080).
        // A function key is f1 to f12 as written, not f+5 or f05, which a
        // number parse would read as 5.
        for text in ["", " ", "ab", "home", "f0", "f13", "f+5", "f05", "👍🏻", "🇺🇸", "İ", "\u{80}"] {
            #expect(TermKey.combination([.ctrl], text) == nil, "\(text)")
        }
        #expect(TermKey.combination([.alt], "İ") == "alt+İ")
    }

    @Test func aCombinationShowsItsModifiersAsSymbols() {
        func key(_ name: String) -> TermKey { TermKey(keys: [name]) }
        #expect(key("ctrl+b").label() == "^B" && key("ctrl+b").spoken == "Control B")
        #expect(key("ctrl+alt+b").label() == "⌃⌥B" && key("shift+b").label() == "⇧B")
        // alt keeps the letter's case, which the terminal receives.
        #expect(key("alt+x").label() == "⌥x" && key("alt+x").spoken == "Alt x")
        #expect(key("alt+X").label() == "⌥X" && key("alt+X").spoken == "Alt X")
        #expect(key("alt+ß").label() == "⌥ß" && key("ctrl+ß").label() == "^ß")
        #expect(key("ctrl+alt+f5").label(symbols: true) == "⌃⌥F5" && key("ctrl+alt+f5").spoken == "Control Alt F5")
        #expect(key("alt+tab").label() == "⌥⇥" && key("alt+tab").spoken == "Alt Tab")
        #expect(key("ctrl+space").label() == "⌃␣" && key("ctrl+space").spoken == "Control Space")
        #expect(key("q").label() == "q" && key("q").spoken == "q")
        // Made by the person, unlike ^C, which More Keys offers.
        #expect(!key("ctrl+b").isStock && key("ctrl+c").isStock && key("ctrl+b").repeats)
    }

    @Test func everyCombinationIsOneKeyTheDaemonAccepts() {
        let modifiers: [TermInput.Modifiers] = [[], [.ctrl], [.alt], [.shift], [.ctrl, .alt, .shift]]
        let texts = ["b", "B", "1", "+", "-", "/", "`", "~", "|", "é", "ß", "İ", "tab", "enter", "esc", "space", "backspace", "up", "down", "left", "right", "f1", "f12"]
        for held in modifiers {
            for text in texts {
                guard let key = TermKey.combination(held, text) else {
                    // Only ctrl+İ, whose lowercase is two scalars.
                    #expect(text == "İ" && held.contains(.ctrl), "\(text)")
                    continue
                }
                #expect(daemonAccepts(key), "\(key)")
                #expect(TermInput.pressed(key) == key, "\(key)")
            }
        }
    }

    /// As `valid_key` in crates/sheprd-remote-control/src/web/terminal.rs.
    private func daemonAccepts(_ key: String) -> Bool {
        let named: Set = ["backspace", "backtick", "down", "enter", "esc", "left", "minus", "plus", "right", "slash", "space", "tab", "up"]
        let parts = key.split(separator: "+", omittingEmptySubsequences: false).map(String.init)
        let last = parts.last ?? ""
        let function = last.hasPrefix("f") && Int(last.dropFirst()).map { (1...12).contains($0) } == true
        // One Rust `char`: a Unicode scalar, not a grapheme.
        let scalar = last.unicodeScalars.count == 1 ? last.unicodeScalars.first : nil
        let single = scalar.map { $0.properties.generalCategory != .control && !$0.properties.isWhitespace } ?? false
        return (1...32).contains(key.utf8.count) && parts.dropLast().allSatisfy { ["ctrl", "alt", "shift"].contains($0) } && (named.contains(last) || single || function)
    }

    @Test func everyChoiceIsOneKeyTheDaemonAccepts() {
        for choice in TermKey.choices {
            #expect(daemonAccepts(choice.key), "\(choice.key)")
            // Already in the order the key row writes modifiers.
            #expect(TermInput.pressed(choice.key) == choice.key)
        }
        #expect(Set(TermKey.choices.map(\.key)).count == TermKey.choices.count)
        let defaults = (TermKeyLayout.defaults.top + TermKeyLayout.defaults.bottom + TermKeyLayout.defaults.spare).filter { $0.special == nil }
        #expect(defaults.allSatisfy { $0.keys.allSatisfy { key in TermKey.choices.contains { $0.key == key } } })
    }
}
