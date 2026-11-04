use super::*;
use crate::code_server::{
    Launcher,
    launcher::Cli,
    supervisor::{Address, Report, Status},
};

type Edits = Arc<Mutex<Vec<CodeEdit>>>;

/// The Code page with VS Code as `cli` says, its saves recorded.
fn start_page(
    cx: &mut TestAppContext,
    cli: Cli,
) -> (Entity<SettingsWindow>, &mut VisualTestContext, Edits) {
    cx.update(|cx| Launcher::fixture(cx, cli));
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    let recorded = edits.clone();
    cx.simulate_resize(size(px(960.), px(1200.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.code.io = Some(CodeIo {
                write: Arc::new(move |edit| {
                    recorded.lock().unwrap().push(edit);
                    Ok(())
                }),
                load: skill_load,
            });
            view.select_section(Section::Code, window, cx);
        })
    });
    draw(cx);
    (view, cx, edits)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(at, Modifiers::default());
    cx.run_until_parked();
    draw(cx);
}

fn found() -> Cli {
    Cli::Found("/Applications/code-tunnel".into())
}

#[gpui::test]
fn a_found_vs_code_is_started_once_its_license_is_accepted(cx: &mut TestAppContext) {
    let (view, cx, edits) = start_page(cx, found());
    view.read_with(cx, |view, cx| {
        assert_eq!(view.code_mode(cx), Some(CodeMode::Start));
    });
    assert!(cx.debug_bounds("settings-code-start").is_some());
    assert!(cx.debug_bounds("settings-code-license").is_some());
    // The address and its check are for the other choice.
    assert!(cx.debug_bounds("settings-code-url").is_none());
    assert!(cx.debug_bounds("settings-code-test").is_none());
    click(cx, "settings-code-accept");
    assert_eq!(edits.lock().unwrap().as_slice(), [CodeEdit::AcceptLicense]);
}

#[gpui::test]
fn choosing_an_address_saves_it_and_stops_vs_code(cx: &mut TestAppContext) {
    let (view, cx, edits) = start_page(cx, found());
    cx.update(|_, cx| {
        let code = crate::config::CodeConfig {
            license_accepted: true,
            ..Default::default()
        };
        Launcher::want(cx, &code);
        assert!(Launcher::running(cx));
    });
    click(cx, "settings-code-address");
    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [CodeEdit::Mode(CodeMode::Address)]
    );
    // The windows stop it once their config says so.
    cx.update(|_, cx| {
        let code = crate::config::CodeConfig {
            mode: Some(CodeMode::Address),
            license_accepted: true,
            ..Default::default()
        };
        Launcher::sync(cx, &code);
    });
    assert!(!cx.update(|_, cx| Launcher::running(cx)));
    // Shown as chosen while the save reloads.
    view.update(cx, |view, cx| {
        view.config.code.mode = Some(CodeMode::Address);
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("settings-code-url").is_some());
    assert!(cx.debug_bounds("settings-code-license").is_none());

    click(cx, "settings-code-start");
    assert_eq!(
        edits.lock().unwrap().last(),
        Some(&CodeEdit::Mode(CodeMode::Start))
    );
}

#[gpui::test]
fn without_vs_code_only_an_address_can_be_chosen(cx: &mut TestAppContext) {
    let (view, cx, edits) = start_page(cx, Cli::Missing);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.code_mode(cx), Some(CodeMode::Address));
    });
    assert!(cx.debug_bounds("settings-code-url").is_some());
    click(cx, "settings-code-start");
    assert!(edits.lock().unwrap().is_empty(), "disabled");
}

#[gpui::test]
fn the_server_state_names_its_port_and_never_its_token(cx: &mut TestAppContext) {
    let (view, cx, _) = start_page(cx, found());
    view.update(cx, |view, cx| {
        view.config.code.license_accepted = true;
        cx.notify();
    });
    cx.update(|_, cx| {
        Launcher::set_report(
            cx,
            Some(Report {
                revision: 1,
                status: Status::Ready,
                address: Some(Address {
                    port: std::num::NonZeroU16::new(51234).unwrap(),
                    url: WebUrl::try_from("http://127.0.0.1:51234/?tkn=secret").unwrap(),
                }),
            }),
        );
    });
    draw(cx);
    assert!(cx.debug_bounds("settings-code-state").is_some());
    assert!(cx.debug_bounds("settings-code-license").is_none());
    let shown = cx.update(
        |_, cx| match Launcher::startup(cx, &view.read(cx).config.code) {
            code_server::Startup::Ready { url } => format!("Running on {}", url.address()),
            other => panic!("{other:?}"),
        },
    );
    assert_eq!(shown, "Running on 127.0.0.1:51234");
}

/// A choice made while another save runs is saved after it, not dropped.
#[gpui::test]
fn choices_made_during_another_save_are_saved_after_it(cx: &mut TestAppContext) {
    let (view, cx, edits) = start_page(cx, found());
    view.update(cx, |view, cx| {
        view.saving = true;
        view.choose_code_mode(CodeMode::Address, cx);
        view.choose_code_mode(CodeMode::Start, cx);
        view.accept_code_license(cx);
    });
    assert!(edits.lock().unwrap().is_empty(), "waiting");
    view.update(cx, |view, cx| {
        view.saving = false;
        view.flush_code_url(cx);
    });
    cx.run_until_parked();
    // The later mode replaced the earlier one; each save ends with a reload
    // that flushes the next.
    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [CodeEdit::Mode(CodeMode::Start), CodeEdit::AcceptLicense]
    );
}
