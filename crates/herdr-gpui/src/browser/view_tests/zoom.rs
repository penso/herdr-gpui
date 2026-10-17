use super::*;
use crate::{browser::TabId, config::FONT_SIZE_STEP, controls::Command};

fn run(view: &Entity<HerdrWindow>, command: Command, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.command(command, window, cx));
    });
    draw(cx);
}

fn zoom(cx: &mut VisualTestContext) -> u16 {
    cx.update(|_, cx| {
        cx.global::<Store>()
            .get(TabId::test(0))
            .unwrap()
            .zoom
            .percent()
    })
}

#[gpui::test]
fn the_font_size_keys_zoom_the_page_the_group_in_use_shows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let font = |cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.config.terminal.size);
    let size = font(cx);

    // A blank tab needs no page, and keeps the zoom its page will take.
    run(&view, Command::NewBrowserTab, cx);
    assert!(
        cx.debug_bounds("browser-zoom").is_none(),
        "100% shows no mark"
    );
    run(&view, Command::IncreaseFontSize, cx);
    run(&view, Command::IncreaseFontSize, cx);
    assert_eq!(zoom(cx), 125);
    assert_eq!(font(cx), size, "the terminal font stays");
    assert!(cx.debug_bounds("browser-zoom").is_some());
    run(&view, Command::DecreaseFontSize, cx);
    assert_eq!(zoom(cx), 110);
    run(&view, Command::ResetFontSize, cx);
    assert_eq!(zoom(cx), 100);
    assert_eq!(font(cx), size);

    // The toolbar's mark returns the page to 100%.
    run(&view, Command::DecreaseFontSize, cx);
    assert_eq!(zoom(cx), 90);
    let mark = cx.debug_bounds("browser-zoom").unwrap();
    cx.simulate_click(mark.center(), gpui::Modifiers::none());
    draw(cx);
    assert_eq!(zoom(cx), 100);
    assert!(cx.debug_bounds("browser-zoom").is_none());

    // Back on the terminal, the keys size its font, and the tab keeps its
    // zoom for when it shows again.
    run(&view, Command::IncreaseFontSize, cx);
    let herdr_tab = cx.debug_bounds("tab-t0").unwrap();
    cx.simulate_click(herdr_tab.center(), gpui::Modifiers::none());
    draw(cx);
    assert!(cx.debug_bounds("terminal").is_some());
    run(&view, Command::IncreaseFontSize, cx);
    assert_eq!(font(cx), size + FONT_SIZE_STEP);
    assert_eq!(zoom(cx), 110);
    let browser_tab = cx.debug_bounds("browser-tab-0").unwrap();
    cx.simulate_click(browser_tab.center(), gpui::Modifiers::none());
    draw(cx);
    assert!(cx.debug_bounds("browser-zoom").is_some());
    run(&view, Command::ResetFontSize, cx);
    assert_eq!(zoom(cx), 100);
    assert_eq!(font(cx), size + FONT_SIZE_STEP, "only the page resets");
}
