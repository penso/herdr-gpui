//! A page closed where this window's strips were not at hand, as another
//! window on the same workspace closes it.
use super::*;

/// Another window closes page `id`, and this window's next tick finds out.
fn close_elsewhere(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    id: crate::browser::TabId,
) {
    cx.update(|window, cx| {
        Store::update(cx, |store| store.close(id));
        view.update(cx, |view, cx| view.poll_browser(window, cx));
    });
    draw(cx);
}

fn split_with_pages(
    cx: &mut TestAppContext,
    count: usize,
) -> (
    Entity<SheprdWindow>,
    &mut VisualTestContext,
    [GroupId; 2],
    Vec<crate::browser::TabId>,
) {
    let (view, cx) = window(cx);
    open_pages(&view, cx, count);
    let pages = cx.update(|_, cx| view.read(cx).browser_tab_ids(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    for page in &pages {
        move_page(&view, cx, right, *page);
    }
    (view, cx, [left, right], pages)
}

#[gpui::test]
fn a_group_whose_last_page_another_window_closed_closes(cx: &mut TestAppContext) {
    let (view, cx, [left, right], pages) = split_with_pages(cx, 1);
    assert_eq!(tabs(&view, cx, right), [Pick::Page(pages[0])]);
    close_elsewhere(&view, cx, pages[0]);
    assert_eq!(groups(&view, cx), [left]);
}

#[gpui::test]
fn a_group_goes_to_another_tab_when_another_window_closes_the_page_it_shows(
    cx: &mut TestAppContext,
) {
    let (view, cx, [left, right], pages) = split_with_pages(cx, 2);
    assert_eq!(
        view.read_with(cx, |view, _| view.group_pick(right)),
        Some(Pick::Page(pages[1]))
    );
    close_elsewhere(&view, cx, pages[1]);
    assert_eq!(groups(&view, cx), [left, right]);
    assert_eq!(tabs(&view, cx, right), [Pick::Page(pages[0])]);
    assert_eq!(
        view.read_with(cx, |view, _| view.group_pick(right)),
        Some(Pick::Page(pages[0]))
    );
}

#[gpui::test]
fn a_page_a_group_only_listed_closes_elsewhere_without_closing_the_group(cx: &mut TestAppContext) {
    let (view, cx, [left, right], pages) = split_with_pages(cx, 2);
    close_elsewhere(&view, cx, pages[0]);
    assert_eq!(groups(&view, cx), [left, right]);
    assert_eq!(tabs(&view, cx, right), [Pick::Page(pages[1])]);
    // A later tick still leaves the group alone.
    cx.update(|window, cx| view.update(cx, |view, cx| view.poll_browser(window, cx)));
    assert_eq!(groups(&view, cx), [left, right]);
}
