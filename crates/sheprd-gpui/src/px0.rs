//! A local px0 workbench, discovered or started on demand. HTTP and process
//! ownership stay on a worker; windows consume only its latest state.
mod error;
mod server;
mod settings;
mod worker;

use crate::browser::WebUrl;
pub use error::Error;
use gpui::{App, Global};
pub(crate) use worker::State;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Config {
    /// A local workbench to attach to. Unset discovers one, then starts px0.
    pub(crate) url: Option<WebUrl>,
}

impl Config {
    pub(crate) fn check(&self) -> Result<(), Error> {
        if let Some(url) = &self.url {
            server::base(url)?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct Launcher {
    /// Authoritative disk configuration, applied by one app-level watcher.
    config: Option<Config>,
    started: bool,
    quitting: bool,
    watch: Option<gpui::Task<()>>,
    worker: Option<worker::Worker>,
    state: State,
}

impl Global for Launcher {}

impl Launcher {
    #[cfg(test)]
    pub(crate) fn fixture(cx: &mut App, config: Config, state: State) {
        cx.set_global(Self {
            config: Some(config),
            started: true,
            state,
            ..Default::default()
        });
    }

    pub(crate) fn install(cx: &mut App) {
        Self::initialize(cx);
        if crate::browser::EMBEDDED {
            let watch = settings::watch(cx);
            cx.global_mut::<Self>().watch = Some(watch);
        }
    }

    fn initialize(cx: &mut App) {
        cx.set_global(Self::default());
        cx.on_app_quit(|cx| {
            let launcher = cx.global_mut::<Self>();
            launcher.quitting = true;
            launcher.watch = None;
            let worker = launcher.worker.take();
            cx.background_executor().spawn(async move {
                if let Some(worker) = worker {
                    worker.shutdown();
                }
            })
        })
        .detach();
    }

    pub(crate) fn state(cx: &App) -> State {
        cx.try_global::<Self>().map_or(State::Idle, |launcher| {
            if !launcher.started {
                return State::Idle;
            }
            launcher
                .worker
                .as_ref()
                .map_or_else(|| launcher.state.clone(), worker::Worker::state)
        })
    }

    /// Only the serialized app-level loader supplies configuration. Windows
    /// may postpone their own reloads while editing or previewing a theme.
    fn apply_config(cx: &mut App, config: Config) {
        let Some(launcher) = cx.try_global::<Self>() else {
            return;
        };
        if launcher.quitting || launcher.config.as_ref() == Some(&config) {
            return;
        }
        Self::reset(cx);
        cx.global_mut::<Self>().config = Some(config);
        cx.refresh_windows();
    }

    pub(crate) fn want(cx: &mut App) {
        if !crate::browser::EMBEDDED || !cx.has_global::<Self>() {
            return;
        }
        let launcher = cx.global_mut::<Self>();
        if launcher.started || launcher.quitting {
            return;
        }
        let Some(config) = &launcher.config else {
            return;
        };
        launcher.started = true;
        match worker::Worker::start(config.clone(), launcher.worker.take()) {
            Ok(worker) => launcher.worker = Some(worker),
            Err(error) => launcher.state = State::Failed(std::sync::Arc::new(error)),
        }
    }

    pub(crate) fn reset(cx: &mut App) {
        if cx.has_global::<Self>() {
            let launcher = cx.global_mut::<Self>();
            launcher.started = false;
            launcher.state = State::Idle;
            if let Some(worker) = &launcher.worker {
                worker.stop();
            }
        }
    }
}

/// A fresh workspace page, with px0 doing its own opening and error handling.
pub(crate) fn workspace_url(base: &WebUrl, root: &str) -> Result<WebUrl, Error> {
    let mut url = server::base(base)?;
    url.query_pairs_mut().append_pair("open", root);
    WebUrl::try_from(url.as_str()).map_err(|_| Error::Address)
}

#[cfg(test)]
mod tests;
