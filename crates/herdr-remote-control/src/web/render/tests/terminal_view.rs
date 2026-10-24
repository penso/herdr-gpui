//! The terminal page: pane switching, the key rows, and the action sheet.

use super::*;
use crate::hub::TerminalPane;

fn pane(id: &str, focused: bool) -> TerminalPane {
    TerminalPane {
        pane: id.into(),
        label: None,
        cwd: Some("/code/herdr-gpui".into()),
        focused,
    }
}

#[test]
fn a_terminal_page_shows_its_screen_keys_and_sheet() {
    let workspace = workspace(Vec::new());
    let mut tab = workspace.terminals[0].clone();
    tab.panes = vec![pane("w1:p1", false), pane("w1:p2", true)];
    let screen = screen("\x1b[32mok\x1b[0m");
    let html = terminal(&workspace, &tab, &tab.panes[1], &screen).into_string();
    assert!(html.contains(r#"data-pane="w1:p2""#), "{html}");
    assert!(html.contains("pane 2 of 2"), "{html}");
    // The other pane is a tap away, its id escaped into the path.
    assert!(html.contains(r#"href="/term/w1%3Ap1""#), "{html}");
    assert!(
        html.contains(r#"id="term-screen""#) && html.contains(r#"<span class="t-fg2">ok</span>"#)
    );
    assert!(html.contains(r#"data-key="ctrl+c""#));
    assert!(html.contains(r#"data-mod="ctrl""#) && html.contains(r#"data-mod="alt""#));
    for action in [
        "split_right",
        "split_down",
        "zoom",
        "close_pane",
        "new_tab",
        "close_tab",
    ] {
        assert!(
            html.contains(&format!(r#"data-term-action="{action}""#)),
            "{action}"
        );
    }
    assert!(html.contains(r#"id="term-input""#));
    assert!(html.contains("Pane 1") && html.contains("Pane 2"), "{html}");
}

#[test]
fn a_numbered_tab_reads_as_tab_n() {
    let workspace = workspace(Vec::new());
    let mut tab = workspace.terminals[0].clone();
    tab.label = "3".into();
    assert!(terminal_page(&workspace, &tab).contains("<b class=\"truncate\">Tab 3</b>"));
    tab.label = "zsh".into();
    assert!(terminal_page(&workspace, &tab).contains("<b class=\"truncate\">zsh</b>"));
}

#[test]
fn a_single_pane_shows_no_switcher() {
    let workspace = workspace(Vec::new());
    let tab = workspace.terminals[0].clone();
    let html = terminal_page(&workspace, &tab);
    assert!(!html.contains("pane 1 of"), "{html}");
    assert!(!html.contains(r#"aria-label="Panes""#), "{html}");
}

fn terminal_page(workspace: &Workspace, tab: &crate::hub::Terminal) -> String {
    terminal(workspace, tab, &tab.panes[0], &screen("$ ")).into_string()
}
