use super::*;
use crate::review::highlight::Token;

#[test]
fn diffs_keep_syntax_tokens_and_original_unicode_offsets() {
    let source = "--- a/example.rs\n+++ b/example.rs\n@@ -1 +1 @@\n-\tlet α = 2;\n+\tlet α = 3;\n";
    let spans = highlight(source);
    assert!(spans[..3].iter().all(Vec::is_empty));
    for (index, number) in [(3, "2"), (4, "3")] {
        let line = source.lines().nth(index).unwrap_or("");
        assert!(spans[index].iter().all(|span| span.start >= 1));
        assert!(spans[index].iter().any(|span| {
            span.token == Token::Number && line.get(span.start..span.end) == Some(number)
        }));
    }
}

#[test]
fn removed_multiline_comments_do_not_color_the_added_side() {
    let source = "*** Begin Patch\n*** Update File: example.rs\n@@\n-/* old comment\n+let value = 5;\n*** End Patch\n";
    let spans = highlight(source);
    assert!(spans[3].iter().any(|span| span.token == Token::Comment));
    assert!(spans[4].iter().any(|span| span.token == Token::Number));
    assert!(!spans[4].iter().any(|span| span.token == Token::Comment));
    assert!(highlight(&"x".repeat(super::super::MAX_CODE_BYTES + 1)).is_empty());
}

#[test]
fn no_newline_annotations_preserve_each_sides_string_context() {
    let source = "--- a/example.rs\n+++ b/example.rs\n@@ -1,2 +1,2 @@\n let value = \"first\n-old last\";\n\\ No newline at end of file\n+new last\";\n\\ No newline at end of file\n";
    let spans = highlight(source);
    assert!(
        spans[6]
            .iter()
            .any(|span| span.token == Token::String && span.start == 1 && span.end >= 4)
    );
    assert!(spans[5].is_empty() && spans[7].is_empty());
}
