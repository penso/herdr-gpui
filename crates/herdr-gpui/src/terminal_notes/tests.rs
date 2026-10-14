use super::*;

fn note(quote: &str, comment: &str) -> Note {
    Note::new(Some("w0:p1".into()), "api / server", quote, comment).expect("note")
}

#[test]
fn empty_comments_and_selections_are_refused() {
    assert!(Note::new(None, "", "text", " \n\t ").is_none());
    assert!(Note::new(None, "", " \n \n", "why?").is_none());
}

#[test]
fn quotes_keep_indentation_and_lose_controls_and_blank_edges() {
    let note = note(
        "\n\n  fn main() {\x1b[31m\r\n\tcall();\u{202e}  \n  }\n\n",
        "Why\nhere?",
    );
    assert_eq!(note.quote, "  fn main() {[31m\n call();\n  }");
    assert_eq!(note.comment, "Why here?");
}

#[test]
fn long_selections_are_cut() {
    let quote = (0..100)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let note = note(&quote, "too long");
    assert_eq!(note.quote.lines().count(), MAX_QUOTED_LINES + 1);
    assert!(note.quote.ends_with("\n\u{2026}"));
    let wide = note_line("x".repeat(1000));
    assert_eq!(wide.chars().count(), MAX_QUOTED_LINE_CHARS + 1);
}

fn note_line(line: String) -> String {
    note(&line, "wide").quote
}

#[test]
fn prompt_fences_quotes_as_data() {
    // The fence outruns any backtick run in the quote, so terminal output
    // holding a fence cannot close the quote and become instructions.
    assert_eq!(fence("failed to connect"), "```");
    assert_eq!(fence("a ``` fence\nline two"), "````");
    let prompt = prompt(&[
        note("failed to connect", "Check the database first."),
        note("a ``` fence\nline two", "Why twice?"),
    ]);
    assert!(prompt.contains("```text\n   failed to connect\n   ```"));
    assert!(prompt.contains("````text\n   a ``` fence\n   line two\n   ````"));
}
