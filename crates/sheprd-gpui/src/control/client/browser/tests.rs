use super::*;

mod zoom;

fn caller(pane_id: Option<&str>) -> Caller {
    Caller {
        pane_id: pane_id.map(str::to_owned),
        workspace_id: Some("w_1".into()),
        daemon_socket: None,
        window: None,
    }
}

#[test]
fn a_file_from_a_pane_opens_beside_it() {
    let file = Page::File("/srv/mockup.html".into());
    assert_eq!(
        default_placement(&file, &caller(Some("w_1:p1"))),
        Placement::Beside
    );
}

#[test]
fn web_addresses_and_files_from_outside_a_pane_take_a_tab() {
    let url = Page::Url("https://a.test/".into());
    assert_eq!(
        default_placement(&url, &caller(Some("w_1:p1"))),
        Placement::Tab
    );
    let file = Page::File("/srv/mockup.html".into());
    assert_eq!(default_placement(&file, &caller(None)), Placement::Tab);
}
