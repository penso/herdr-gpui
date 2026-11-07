use super::*;
use crate::controls::Command;

fn run(view: &Entity<SheprdWindow>, command: Command, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.command(command, window, cx));
    });
    draw(cx);
}

fn flash(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, _| {
        view.flash.as_ref().map(|(flash, _)| flash.text.to_string())
    })
}

/// With the terminal in view there is no page, so the binding says so
/// rather than doing nothing; a build without pages says why.
#[gpui::test]
fn toggle_annotate_without_a_page_says_so(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    run(&view, Command::ToggleAnnotate, cx);
    let expected = if crate::browser::EMBEDDED {
        "No page to annotate"
    } else {
        "Annotating pages needs macOS or Windows"
    };
    assert_eq!(flash(&view, cx).as_deref(), Some(expected));
}

/// The binding starts and stops annotating the page in view, as its toolbar
/// button does, and a blank tab has nothing to annotate yet.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn toggle_annotate_arms_and_disarms_the_page_in_view(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    run(&view, Command::NewBrowserTab, cx);
    run(&view, Command::ToggleAnnotate, cx);
    assert_eq!(flash(&view, cx).as_deref(), Some("No page to annotate"));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let url = WebUrl::try_from("http://localhost:3000/").unwrap();
            view.open_browser_tab(Some(url), window, cx);
        });
    });
    draw(cx);
    let id = cx.update(|_, cx| {
        cx.global::<Store>()
            .tabs()
            .find(|tab| tab.location.is_some())
            .unwrap()
            .id
    });
    let armed = |view: &Entity<SheprdWindow>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| view.browser.annotations.armed(id))
    };
    run(&view, Command::ToggleAnnotate, cx);
    assert!(armed(&view, cx));
    run(&view, Command::ToggleAnnotate, cx);
    assert!(!armed(&view, cx));
}
