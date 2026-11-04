use super::*;

#[test]
fn a_terminal_offers_paste_and_a_picture() {
    let workspace = workspace(Vec::new());
    let tab = workspace.terminals[0].clone();
    let html = terminal(&workspace, &tab, &tab.panes[0], &screen("$ ")).into_string();
    for marker in ["data-term-paste", "data-term-picture", r#"id="term-file""#] {
        assert!(html.contains(marker), "{marker}");
    }
}
