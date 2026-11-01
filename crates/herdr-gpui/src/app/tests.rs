use super::{Config, InitialAppearance, StartupAppearance, Theme};
#[cfg(feature = "integration-test")]
use super::{ConnectTarget, HerdrWindow};
use crate::config::LayoutMode;
#[cfg(feature = "integration-test")]
use gpui::px;

#[test]
fn startup_appearance_loads_a_coherent_pair_and_reports_errors() {
    let appearance = InitialAppearance::load(
        || {
            let mut config = Config {
                theme: "Nord".into(),
                ..Default::default()
            };
            config.layout.mode = LayoutMode::from(crate::config::Density::Compact);
            Ok(config)
        },
        false,
    );
    assert_eq!(
        appearance.config.layout.mode,
        LayoutMode::from(crate::config::Density::Compact)
    );
    assert_eq!(Some(appearance.theme), Theme::builtin("Nord"));
    assert!(appearance.error.is_none());
    for appearance in [
        InitialAppearance::load(|| Err(crate::Error::MissingHome), false),
        InitialAppearance::load(
            || {
                Ok(Config {
                    theme: "../invalid".into(),
                    ..Default::default()
                })
            },
            false,
        ),
    ] {
        assert_eq!(
            appearance.config.layout.mode,
            LayoutMode::from(crate::config::Density::Normal)
        );
        assert_eq!(appearance.theme, Theme::default());
        assert!(appearance.error.is_some());
    }
}

#[test]
fn only_a_plain_launch_hands_off_to_the_running_app() {
    use super::{ConnectTarget, focus_running};
    use crate::cli::LaunchMode;
    use std::process::ExitCode;
    let focused = || Ok(String::from("Brought Herdr UI forward"));
    let local = ConnectTarget::Local;
    assert_eq!(
        focus_running(&LaunchMode::Normal, &local, focused),
        Some(ExitCode::SUCCESS)
    );
    // Another target, or another mode, is never handed off, nor asked.
    let session = ConnectTarget::Session {
        name: "work".into(),
        development: false,
    };
    let unasked = || -> crate::Result<String> { panic!("asked the running app") };
    assert_eq!(focus_running(&LaunchMode::Normal, &session, unasked), None);
    assert_eq!(focus_running(&LaunchMode::Help, &local, unasked), None);
    assert_eq!(focus_running(&LaunchMode::BuildInfo, &local, unasked), None);
    // No app answers, or the one that does cannot come forward: start one.
    let unavailable = || {
        Err(crate::control::Error::Unavailable {
            path: "pipe".into(),
            source: std::io::ErrorKind::NotFound.into(),
        }
        .into())
    };
    assert_eq!(
        focus_running(&LaunchMode::Normal, &local, unavailable),
        None
    );
    let refused = || {
        Err(crate::control::Error::Rejected {
            code: crate::control::ErrorCode::NoWindow,
            message: "Herdr UI has no window".into(),
        }
        .into())
    };
    assert_eq!(focus_running(&LaunchMode::Normal, &local, refused), None);
    let silent = || Err(crate::control::Error::NoResponse.into());
    assert_eq!(focus_running(&LaunchMode::Normal, &local, silent), None);
}

#[test]
fn startup_resolves_both_sides_so_the_first_frame_needs_no_disk_read() {
    let load = || {
        StartupAppearance::load(|| {
            Ok(Config {
                theme: "light:Catppuccin Latte,dark:Nord".into(),
                ..Default::default()
            })
        })
    };
    assert_eq!(
        Some(load().select(true).theme),
        Theme::builtin("Catppuccin Latte")
    );
    assert_eq!(Some(load().select(false).theme), Theme::builtin("Nord"));
    let single = StartupAppearance::load(|| {
        Ok(Config {
            theme: "Nord".into(),
            ..Default::default()
        })
    });
    assert!(single.light.is_none());
    assert_eq!(Some(single.select(true).theme), Theme::builtin("Nord"));
}

#[cfg(feature = "integration-test")]
#[gpui::test]
fn first_window_frame_uses_startup_layout(cx: &mut gpui::TestAppContext) {
    use crate::config::{Density, Style};
    for mode in [Density::Compact, Density::Normal, Density::Comfortable]
        .into_iter()
        .flat_map(|density| {
            [Style::Flat, Style::Rounded].map(|style| LayoutMode::new(density, style))
        })
    {
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut appearance = InitialAppearance::load(
                || {
                    Ok(Config {
                        theme: "Nord".into(),
                        ..Default::default()
                    })
                },
                false,
            );
            appearance.config.layout.mode = mode;
            cx.set_global(appearance);
            HerdrWindow::new(
                ConnectTarget::Socket("/unused-startup.sock".into()),
                window,
                cx,
                true,
            )
        });
        // Draw immediately, without polling a background config completion.
        cx.update(|window, cx| {
            let state = view.read(cx);
            assert_eq!(state.config.layout.mode, mode);
            assert_eq!(Some(state.theme.clone()), Theme::builtin("Nord"));
            assert!(state.config_load.is_none());
            crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
        });
        let row = cx
            .debug_bounds("row-herdr")
            .unwrap_or_else(|| panic!("missing first-frame row"));
        // Rounded rows add padding inside their highlight and spacing
        // around it: a third of the density's gap, each, twice.
        assert_eq!(
            row.size.height,
            px(match (mode.density(), mode.style()) {
                (Density::Compact, Style::Flat) => 16.,
                (Density::Normal, Style::Flat) => 32.,
                (Density::Comfortable, Style::Flat) => 40.,
                (Density::Compact, Style::Rounded) => 16. + 2. + 2.,
                (Density::Normal, Style::Rounded) => 32. + 4. + 4.,
                (Density::Comfortable, Style::Rounded) => 40. + 6. + 6.,
            })
        );
    }
}
