//! Azure Boards descriptions keep their paragraphs and links as Markdown.
use crate::{
    Provider, SourceKey,
    azure::{html::to_markdown, work_item::WorkItemNode},
};

#[test]
fn blocks_become_paragraphs_and_breaks_become_lines() {
    assert_eq!(
        to_markdown("<div>First</div><div>Second<br>line</div>"),
        "First\n\nSecond\nline"
    );
    assert_eq!(to_markdown("<P>One</P>\n  <P>Two</P>"), "One\n\nTwo");
}

#[test]
fn lists_keep_their_items() {
    assert_eq!(
        to_markdown("<p>Steps</p><ul><li>One</li><li>Two</li></ul>"),
        "Steps\n\n- One\n- Two"
    );
}

#[test]
fn web_links_keep_their_address() {
    assert_eq!(
        to_markdown(r#"See <a class="x" href="https://example.test/a?b=1&amp;c=2">the doc</a>."#),
        "See [the doc](https://example.test/a?b=1&c=2)."
    );
    assert_eq!(
        to_markdown("<A HREF='http://example.test'>Upper</A>"),
        "[Upper](http://example.test)"
    );
}

#[test]
fn only_web_addresses_become_links() {
    assert_eq!(
        to_markdown(r#"<a href="javascript:alert(1)">Click</a>"#),
        "Click"
    );
    assert_eq!(
        to_markdown(r#"<a href="https://example.test/a)b">Odd</a>"#),
        "Odd"
    );
    assert_eq!(to_markdown("<a name=\"top\">Anchor</a>"), "Anchor");
}

#[test]
fn a_link_around_no_text_is_dropped_and_an_open_one_is_closed() {
    assert_eq!(
        to_markdown(r#"<a href="https://example.test"><img src="x.png"></a>"#),
        ""
    );
    assert_eq!(
        to_markdown(r#"<a href="https://example.test">Open"#),
        "[Open](https://example.test)"
    );
}

#[test]
fn hidden_content_and_comments_are_dropped() {
    assert_eq!(
        to_markdown("<style>p { color: red }</style><!-- note --><script>x()</SCRIPT>Text"),
        "Text"
    );
    assert_eq!(to_markdown("<div></div><p> </p>"), "");
}

#[test]
fn entities_and_bare_brackets_stay_text() {
    assert_eq!(
        to_markdown("<p>&lt;div&gt; &amp; a < b</p>"),
        "&lt;div&gt; &amp; a < b"
    );
    assert_eq!(to_markdown("Unclosed <b"), "Unclosed <b");
}

#[test]
fn preformatted_text_keeps_its_lines() {
    assert_eq!(
        to_markdown("<pre>one\n  two</pre><p>after</p>"),
        "one\n  two\n\nafter"
    );
}

#[test]
fn multibyte_text_survives_tags() {
    assert_eq!(
        to_markdown("<p>caf\u{e9}</p><p>\u{65e5}\u{672c}</p>"),
        "caf\u{e9}\n\n\u{65e5}\u{672c}"
    );
}

#[test]
fn a_work_item_stores_its_description_as_markdown() {
    let node = |description: &str| -> WorkItemNode {
        serde_json::from_value(serde_json::json!({
            "id": 7,
            "fields": {
                "System.Title": "Title",
                "System.State": "Active",
                "System.Description": description,
            },
        }))
        .unwrap()
    };
    let source = SourceKey {
        provider: Provider::Azure,
        host: "dev.azure.com".into(),
        repository: "contoso/Fabrikam".into(),
    };
    let web = "https://dev.azure.com/contoso/Fabrikam";
    let item = node("<div>One</div><div>Two</div>").into_item(&source, web);
    assert_eq!(item.description.as_deref(), Some("One\n\nTwo"));
    let empty = node("<div> </div>").into_item(&source, web);
    assert_eq!(empty.description, None);
}

#[test]
fn a_bracket_before_another_opens_no_tag() {
    assert_eq!(to_markdown("<a <b>x"), "<a x");
}

/// Quadratic scanning would not finish this in any reasonable test run.
#[test]
fn many_unclosed_brackets_are_read_once() {
    let brackets = "<".repeat(1_000_000);
    let html = format!("{brackets}>");
    assert_eq!(to_markdown(&html), html);
    let tags = "<a ".repeat(300_000);
    // Only the last `<a >` closes, so it alone is a tag.
    let last_tag_and_space = "<a  ".len();
    assert_eq!(
        to_markdown(&format!("{tags}>")).len(),
        tags.len() - last_tag_and_space
    );
}
