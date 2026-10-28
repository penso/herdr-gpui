use super::*;

#[test]
fn issue_context_names_the_tab_and_pane_before_the_long_prompt_is_pasted() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    perform(
        &site,
        &Action::Dispatch(Box::new(request())),
        &AtomicBool::new(false),
    )
    .unwrap();
    let calls = calls(dir.path());
    let tab = calls
        .iter()
        .position(|call| call == "tab rename t9 #377 Fix the icons")
        .unwrap();
    let pane = calls
        .iter()
        .position(|call| call == "pane rename p9 #377 Fix the icons")
        .unwrap();
    let prompt = calls
        .iter()
        .position(|call| call.starts_with("agent prompt p9 "))
        .unwrap();
    assert!(tab < prompt && pane < prompt);
}
