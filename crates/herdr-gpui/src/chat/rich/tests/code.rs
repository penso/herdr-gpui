use super::*;

#[test]
fn code_preserves_tabs_in_copy_and_keeps_inline_words_separated() {
    let source = "\tlet α = 2;\n\t\tprintln!(\"{α}\");\n";
    for closing in ["", "```"] {
        let blocks = parse(
            &format!("```rust\n{source}{closing}"),
            &mut Visuals::default(),
        )
        .unwrap();
        assert!(matches!(&blocks[0], Block::Code { source: code, .. } if &**code == source));
    }
    let blocks = parse("Use `left\tright` together.", &mut Visuals::default()).unwrap();
    let Block::Paragraph { content, .. } = &blocks[0] else {
        panic!("paragraph")
    };
    assert!(content.iter().any(
        |part| matches!(part, Inline::Text(text, style) if style.code && text == "left    right")
    ));
}

#[test]
fn a_closed_fence_inside_a_quote_is_highlighted() {
    let mut visuals = Visuals::default();
    for text in [
        "> ```rust\n> fn main() {}\n> ```",
        "> > ```rust\n> > fn main() {}\n> > ```",
        "> - ```rust\n>   fn main() {}\n>   ```",
    ] {
        let blocks = parse(text, &mut visuals).unwrap();
        let id = blocks.iter().find_map(|block| match block {
            Block::Code { id, .. } => Some(*id),
            _ => None,
        });
        assert!(id.is_some_and(|id| visuals.wants(id)), "{text}");
    }
    let open = parse("> ```rust\n> fn main() {}", &mut visuals).unwrap();
    let Some(Block::Code { id, .. }) = open.first() else {
        panic!("code")
    };
    assert!(!visuals.wants(*id));
}
