//! A sign-in that may not read Dependabot alerts says so, rather than
//! passing for a repository without any.
use super::section::{draw, install, window};
use super::*;
use crate::security::fetch::{checked, readable};

#[test]
fn an_empty_listing_rest_refuses_reads_as_hidden() {
    let empty = || open(Vec::new());
    assert_eq!(
        checked(
            empty(),
            Err(crate::Error::GitHub(crate::github::Error::Forbidden))
        ),
        Alerts::Hidden
    );
    assert_eq!(
        checked(
            empty(),
            Err(crate::Error::GitHub(crate::github::Error::Status(404)))
        ),
        Alerts::Hidden
    );
    assert_eq!(checked(empty(), Ok(())), empty());
    // A failure that is no refusal leaves the listing as GraphQL read it.
    assert_eq!(
        checked(
            empty(),
            Err(crate::Error::GitHub(crate::github::Error::Status(500)))
        ),
        empty()
    );
    assert_eq!(
        checked(
            empty(),
            Err(crate::Error::GitHub(crate::github::Error::RateLimit {
                reset: None
            }))
        ),
        empty()
    );
}

#[test]
fn an_account_that_cannot_read_alerts_is_not_the_best_read() {
    // Another host's sign-in is tried, and kept when it reads them.
    let hidden = repo("a/one", Ok(Alerts::Hidden));
    let read = repo("a/one", Ok(open(Vec::new())));
    assert!(readable(&hidden) < readable(&read));
}

#[gpui::test]
fn the_section_names_repositories_whose_alerts_it_cannot_read(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    install(
        &view,
        cx,
        vec![
            repo("a/hidden", Ok(Alerts::Hidden)),
            repo("a/quiet", Ok(open(Vec::new()))),
        ],
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Security, window, cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("security-section-unreadable").is_some());
    assert!(cx.debug_bounds("security-section-quiet").is_some());
}
