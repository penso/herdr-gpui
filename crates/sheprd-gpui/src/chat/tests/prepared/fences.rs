use super::*;
use core::prelude::v1::test;

const CARD: &str = r#"{"components":[{"type":"text","text":"Example"}]}"#;

#[test]
fn indented_closers_cannot_activate_unfinished_widgets() {
    for marker in ["```", "~~~"] {
        for indentation in ["    ", "\t", "   \t"] {
            let text = format!("{marker}herdr-ui\n{CARD}\n{indentation}{marker}\n");
            let (prepared, _) = visual::message(&text);
            let body = prepared.body(EntryId(1)).unwrap();
            let Body::Message(parts) = body.as_ref() else {
                panic!("message");
            };
            assert!(
                !parts.iter().any(|part| matches!(part, Part::Widget(_))),
                "{text:?}"
            );
        }
    }
}

#[test]
fn indented_outer_closers_keep_nested_widgets_literal() {
    for marker in ["````", "~~~~"] {
        for indentation in ["    ", "\t"] {
            let text = format!(
                "{marker}markdown\n{indentation}{marker}\n```herdr-ui\n{CARD}\n```\n{marker}\n"
            );
            let (prepared, _) = visual::message(&text);
            let body = prepared.body(EntryId(1)).unwrap();
            let Body::Message(parts) = body.as_ref() else {
                panic!("message");
            };
            assert!(
                !parts.iter().any(|part| matches!(part, Part::Widget(_))),
                "{text:?}"
            );
        }
    }
}

#[test]
fn widget_closers_allow_up_to_three_spaces_and_longer_matching_runs() {
    for marker in ["```", "~~~"] {
        for indentation in ["", " ", "  ", "   "] {
            let text = format!(
                "{marker}herdr-ui\r\n{CARD}\r\n{indentation}{marker}{} \t\r\n",
                &marker[..1]
            );
            let (prepared, _) = visual::message(&text);
            let body = prepared.body(EntryId(1)).unwrap();
            let Body::Message(parts) = body.as_ref() else {
                panic!("message");
            };
            assert!(matches!(&parts[..], [Part::Widget(_)]), "{text:?}");
        }
    }
}
