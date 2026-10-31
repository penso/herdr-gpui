import Testing
@testable import HerdrRemoteCore

@Suite struct MarkdownTests {
    @Test func paragraphsSplitOnBlankLines() {
        #expect(MarkdownBlock.parse("One\nstill one\n\nTwo") == [.paragraph("One\nstill one"), .paragraph("Two")])
    }

    @Test func fencedCodeKeepsItsLanguageAndIndentation() {
        let text = "Run:\n```swift\nlet x = 1\n  indented\n```\nDone"
        #expect(MarkdownBlock.parse(text) == [
            .paragraph("Run:"),
            .code(language: "swift", text: "let x = 1\n  indented"),
            .paragraph("Done"),
        ])
    }

    @Test func anUnclosedFenceIsCodeSoFar() {
        // What a streaming answer looks like halfway through a block.
        #expect(MarkdownBlock.parse("```\nlet partial") == [.code(language: nil, text: "let partial")])
    }

    @Test func aLongerFenceNeedsALongerClose() {
        let text = "````\n```\ninner\n```\n````"
        #expect(MarkdownBlock.parse(text) == [.code(language: nil, text: "```\ninner\n```")])
    }

    @Test func headingsDropClosingHashes() {
        #expect(MarkdownBlock.parse("# Title ##\n### Sub") == [
            .heading(level: 1, text: "Title"), .heading(level: 3, text: "Sub"),
        ])
        #expect(MarkdownBlock.parse("#hashtag") == [.paragraph("#hashtag")])
    }

    @Test func listsNestAndTick() {
        let text = "- one\n  - nested\n- [x] done\n- [ ] todo\n\n- after a blank"
        #expect(MarkdownBlock.parse(text) == [
            .list(ordered: false, start: 1, items: [
                MarkdownListItem(text: "one"),
                MarkdownListItem(text: "nested", depth: 1),
                MarkdownListItem(text: "done", checked: true),
                MarkdownListItem(text: "todo", checked: false),
                MarkdownListItem(text: "after a blank"),
            ]),
        ])
    }

    @Test func orderedListsKeepTheirStart() {
        #expect(MarkdownBlock.parse("3. three\n4. four\nNext") == [
            .list(ordered: true, start: 3, items: [MarkdownListItem(text: "three"), MarkdownListItem(text: "four")]),
            .paragraph("Next"),
        ])
    }

    @Test func aListItemContinuesOnIndentedLines() {
        #expect(MarkdownBlock.parse("- first\n  more of it") == [
            .list(ordered: false, start: 1, items: [MarkdownListItem(text: "first\nmore of it")]),
        ])
    }

    @Test func quotesAndRules() {
        #expect(MarkdownBlock.parse("> a\n> b\n\n---\n***") == [.quote("a\nb"), .rule, .rule])
    }

    @Test func tablesSplitCellsOutsideCode() {
        let text = "| Name | Use |\n| :--- | ---: |\n| `a|b` | pipe \\| here |\n| short |"
        #expect(MarkdownBlock.parse(text) == [
            .table(header: ["Name", "Use"], rows: [["`a|b`", "pipe | here"], ["short", ""]]),
        ])
    }

    @Test func aPipeWithoutADelimiterRowIsText() {
        #expect(MarkdownBlock.parse("a | b\nc") == [.paragraph("a | b\nc")])
    }

    @Test func blocksInterruptAParagraph() {
        #expect(MarkdownBlock.parse("Intro\n- item\n# Head") == [
            .paragraph("Intro"),
            .list(ordered: false, start: 1, items: [MarkdownListItem(text: "item")]),
            .heading(level: 1, text: "Head"),
        ])
    }

    @Test func appendingTextOnlyGrowsTheEnd() {
        let full = "Intro\n\n```sh\nmake\n```\n\n- a\n- b"
        var previous: [MarkdownBlock] = []
        for end in full.indices {
            let blocks = MarkdownBlock.parse(String(full[...end]))
            // Every block before the last one stays as it was.
            if previous.count > 1 { #expect(Array(blocks.prefix(previous.count - 1)) == Array(previous.dropLast())) }
            previous = blocks
        }
    }
}
