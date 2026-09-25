//! Process bootstrap and window opening. CLI parsing and the updater helper
//! run before GPUI starts, so an invalid option or `--build-info` exits without
//! ever creating a window.

use crate::{
    APP_VERSION, HerdrWindow, Quit, ShowLogs, WINDOW_TITLE, app_icon, bind_keys, cli,
    config::{Config, Theme},
    diagnostics, icons, log_window, menus, titlebar, updater,
};
#[cfg(feature = "integration-test")]
use crate::{performance, smoke};
use anyhow::Result;
use gpui_kit::{component::Root, prelude::*, *};
use herdr_client::ConnectTarget;

/// A main Herdr window. GPUI's handle names the kit [`Root`], which wraps the
/// view and renders the kit's dialog, sheet and notification layers above it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MainWindow(WindowHandle<Root>);

impl MainWindow {
    /// Updates the Herdr view inside this window, as `WindowHandle::update`
    /// does for a window whose root is the view itself.
    pub(crate) fn update<C: AppContext, R>(
        self,
        cx: &mut C,
        update: impl FnOnce(&mut HerdrWindow, &mut Window, &mut Context<HerdrWindow>) -> R,
    ) -> Result<R> {
        self.0.update(cx, |root, window, cx| {
            let view = root
                .view()
                .clone()
                .downcast::<HerdrWindow>()
                .map_err(|_| anyhow::anyhow!("the window root holds another view"))?;
            Ok(view.update(cx, |view, cx| update(view, window, cx)))
        })?
    }

    /// Finds the Herdr view behind any window, skipping the Logs window.
    pub(crate) fn find(handle: AnyWindowHandle, cx: &App) -> Option<Self> {
        let handle = handle.downcast::<Root>()?;
        handle
            .read(cx)
            .ok()?
            .view()
            .clone()
            .downcast::<HerdrWindow>()
            .ok()?;
        Some(Self(handle))
    }

    /// Every open main window, in GPUI's window order.
    pub(crate) fn all(cx: &App) -> Vec<Self> {
        cx.windows()
            .into_iter()
            .filter_map(|handle| Self::find(handle, cx))
            .collect()
    }
}

impl From<MainWindow> for AnyWindowHandle {
    fn from(window: MainWindow) -> Self {
        window.0.into()
    }
}

/// The Herdr view inside a window's root view, mirroring `AnyView::downcast`.
#[cfg(feature = "integration-test")]
pub(crate) fn herdr_view(
    root: AnyView,
    cx: &App,
) -> std::result::Result<Entity<HerdrWindow>, AnyView> {
    let kit = root.downcast::<Root>()?;
    kit.read(cx).view().clone().downcast::<HerdrWindow>()
}

/// The first frame must not use default density while disk settings load.
/// Load before the UI event loop; later windows reuse the last validated pair.
#[derive(Clone, Default)]
pub(crate) struct InitialAppearance {
    pub config: Config,
    pub theme: Theme,
    pub error: Option<String>,
}

impl Global for InitialAppearance {}

impl InitialAppearance {
    fn load(load: impl FnOnce() -> crate::Result<Config>) -> Self {
        match load().and_then(|config| Ok((config.theme()?, config))) {
            Ok((theme, config)) => Self {
                config,
                theme,
                error: None,
            },
            Err(error) => Self {
                error: Some(format!("Load GUI config: {error}")),
                ..Self::default()
            },
        }
    }
}

/// Opens one main window onto `target`. Every window is an independent client
/// of that daemon: its own connection, surface lease, and workspace focus.
pub(crate) fn open_window(
    target: ConnectTarget,
    updater: updater::Updater,
    cx: &mut App,
    #[cfg(feature = "integration-test")] fixture: bool,
) -> Result<MainWindow> {
    // Cascade rather than stack windows exactly, so a new one is visible at once.
    let existing = MainWindow::all(cx).len();
    let step = px(28. * existing.min(6) as f32);
    let mut bounds = Bounds::centered(None, size(px(1200.), px(780.)), cx);
    bounds.origin += point(step, step);
    let (bounds, display_id) = crate::window_state::WindowState::placement(bounds, cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id,
            window_min_size: Some(size(px(640.), px(400.))),
            titlebar: Some(titlebar::options(WINDOW_TITLE)),
            // The kit title bar moves the window and zooms on double click
            // itself; AppKit must not also treat that strip as its own.
            app_owns_titlebar_drag: cfg!(target_os = "macos"),
            app_id: Some("so.pen.herdr-gpui".into()),
            ..Default::default()
        },
        |window, cx| {
            let herdr = cx.new(|cx| {
                let mut view = HerdrWindow::new(
                    target,
                    window,
                    cx,
                    #[cfg(feature = "integration-test")]
                    fixture,
                );
                view.updater = updater;
                crate::window_state::WindowState::observe(window, cx);
                view
            });
            cx.new(|cx| Root::new(herdr, window, cx))
        },
    )?;
    Ok(MainWindow(handle))
}

/// Opens another window from inside the focused window's own update.
pub(crate) fn open_additional_window(target: ConnectTarget, cx: &mut App) {
    cx.defer(move |cx| {
        let opened = open_window(
            target,
            updater::Updater::secondary(),
            cx,
            #[cfg(feature = "integration-test")]
            false,
        );
        match opened {
            Ok(handle) => {
                let _ = handle.update(cx, |view, window, _| {
                    view.sound = crate::sound::Service::new();
                    window.activate_window();
                });
            }
            Err(_) => tracing::error!("Unable to open an additional Herdr window"),
        }
    });
}

pub(crate) fn run() -> std::process::ExitCode {
    use cli::{LaunchMode, LaunchOptions};
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if let Some(exit) = updater::run_helper(&args) {
        return exit;
    }
    let LaunchOptions { target, mode } = match LaunchOptions::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!(
                "{error}\nUsage: herdr-gpui [--socket CLIENT_SOCKET | --session NAME [--dev]]"
            );
            return std::process::ExitCode::from(2);
        }
    };
    if mode == LaunchMode::BuildInfo {
        print!("{}", cli::build_info());
        return std::process::ExitCode::SUCCESS;
    }
    if mode == LaunchMode::Help {
        println!(
            "herdr-gpui [--socket CLIENT_SOCKET | --session NAME [--dev]]\nConnects to Local and saved SSH hosts; never installs remote software.\nStarts the local Herdr daemon if needed; never stops it.\nExplicit --socket and --dev targets are attach-only; --socket isolates the GUI to one existing daemon."
        );
        println!(
            "  --build-info        Print the executable's build identity without starting the GUI"
        );
        #[cfg(feature = "integration-test")]
        println!(
            "  --integration-test  Run native GUI checks (requires explicit --socket)\n  --sidebar-test      Run native sidebar fixtures without connecting to a daemon\n  --performance-test  Measure native dense-terminal hover/scroll without a daemon (macOS)"
        );
        return std::process::ExitCode::SUCCESS;
    }
    #[cfg(feature = "integration-test")]
    let integration_test = mode == LaunchMode::Integration;
    #[cfg(feature = "integration-test")]
    let sidebar_test = mode == LaunchMode::Sidebar;
    #[cfg(feature = "integration-test")]
    let performance_test = mode == LaunchMode::Performance;
    #[cfg(feature = "integration-test")]
    if mode != LaunchMode::Normal {
        smoke::EXIT_CODE.store(1, std::sync::atomic::Ordering::SeqCst);
    }
    let startup_failed = std::rc::Rc::new(std::cell::Cell::new(false));
    if let Err(error) = diagnostics::init() {
        eprintln!("Unable to initialize diagnostics: {error}");
        return std::process::ExitCode::FAILURE;
    }
    tracing::info!(
        version = APP_VERSION,
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        "GPUI client starting"
    );
    // Only read first-frame settings here. Migration, defaults refresh, and font
    // discovery run after opening; CLI and fixtures skip personal settings.
    let appearance = if mode == LaunchMode::Normal {
        let started = std::time::Instant::now();
        let appearance = InitialAppearance::load(Config::load_startup);
        tracing::debug!(
            elapsed_us = started.elapsed().as_micros() as u64,
            "Startup appearance loaded"
        );
        appearance
    } else {
        InitialAppearance::default()
    };
    let failed = startup_failed.clone();
    let window_state = (mode == LaunchMode::Normal).then(crate::window_state::WindowState::load);
    application().with_assets(icons::Icons).run(move |cx| {
        // The kit registers its themes, keybindings and globals before any
        // kit component renders.
        init(cx);
        crate::kit_theme::sync(&appearance.theme, &appearance.config, cx);
        let window_count = window_state.as_ref().map_or(1, |state| state.count());
        if let Some(state) = window_state {
            state.install(cx);
        }
        cx.set_global(appearance);
        app_icon::install();
        #[cfg(target_os = "macos")]
        crate::app_badge::install(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &ShowLogs, cx| log_window::open(cx));
        bind_keys(cx);
        cx.set_menus(menus());
        cx.on_window_closed(move |cx, _| {
            if cx.windows().is_empty() {
                #[cfg(feature = "integration-test")]
                if performance_test {
                    std::process::exit(1);
                }
                cx.quit();
            }
        })
        .detach();
        // Native test modes and CLI invocations never start an updater worker.
        let updater = if mode == LaunchMode::Normal {
            updater::Updater::start()
        } else {
            updater::Updater::default()
        };
        let opened = open_window(
            target.clone(),
            updater,
            cx,
            #[cfg(feature = "integration-test")]
            {
                sidebar_test || performance_test
            },
        );
        match opened {
            Ok(_window) => {
                if mode == LaunchMode::Normal {
                    let _ = _window.update(cx, |view, _, _| {
                        view.sound = crate::sound::Service::new();
                    });
                    for _ in 1..window_count {
                        open_additional_window(target.clone(), cx);
                    }
                }
                #[cfg(feature = "integration-test")]
                if performance_test {
                    performance::start(_window, cx);
                }
                #[cfg(feature = "integration-test")]
                if integration_test {
                    smoke::start(_window, cx);
                }
                #[cfg(feature = "integration-test")]
                if sidebar_test {
                    smoke::start_sidebar(_window, cx);
                }
            }
            Err(error) => {
                tracing::error!("Unable to open main window");
                eprintln!("Unable to open Herdr window: {error}");
                failed.set(true);
                #[cfg(feature = "integration-test")]
                if performance_test {
                    std::process::exit(1);
                }
                cx.quit();
            }
        }
        cx.activate(true);
    });
    if startup_failed.get() {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, InitialAppearance, Theme};
    #[cfg(feature = "integration-test")]
    use super::{ConnectTarget, HerdrWindow};
    use crate::config::LayoutMode;
    #[cfg(feature = "integration-test")]
    #[test]
    fn startup_appearance_loads_a_coherent_pair_and_reports_errors() {
        let appearance = InitialAppearance::load(|| {
            let mut config = Config {
                theme: "Nord".into(),
                ..Default::default()
            };
            config.layout.mode = LayoutMode::from(crate::config::Density::Compact);
            Ok(config)
        });
        assert_eq!(
            appearance.config.layout.mode,
            LayoutMode::from(crate::config::Density::Compact)
        );
        assert_eq!(Some(appearance.theme), Theme::builtin("Nord"));
        assert!(appearance.error.is_none());
        for appearance in [
            InitialAppearance::load(|| Err(crate::Error::MissingHome)),
            InitialAppearance::load(|| {
                Ok(Config {
                    theme: "../invalid".into(),
                    ..Default::default()
                })
            }),
        ] {
            assert_eq!(
                appearance.config.layout.mode,
                LayoutMode::from(crate::config::Density::Normal)
            );
            assert_eq!(appearance.theme, Theme::default());
            assert!(appearance.error.is_some());
        }
    }

    #[cfg(feature = "integration-test")]
    #[gpui_kit::test]
    fn first_window_frame_uses_startup_layout(cx: &mut gpui_kit::TestAppContext) {
        use crate::config::{Density, Style};
        for mode in [Density::Compact, Density::Normal, Density::Comfortable]
            .into_iter()
            .flat_map(|density| {
                [Style::Flat, Style::Rounded].map(|style| LayoutMode::new(density, style))
            })
        {
            let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
                let mut appearance = InitialAppearance::load(|| {
                    Ok(Config {
                        theme: "Nord".into(),
                        ..Default::default()
                    })
                });
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
            // Kit sidebar rows have one height; the layout reaches the frame
            // through the config checked above.
            assert!(
                cx.debug_bounds("row-herdr").is_some(),
                "missing first-frame row"
            );
        }
    }
}
