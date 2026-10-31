//! Cloud machines as devices: `herdr_remote::cloud`, plus what the window
//! drives here: the background jobs that add machines with their progress
//! and toasts, the batches agents ask for, and the sandbox image builds.

pub(crate) mod batch;
pub(crate) mod image;
mod jobs;
pub(crate) mod status;
pub(crate) mod worker;

pub(crate) use herdr_remote::cloud::*;
pub(crate) use jobs::Jobs;

/// Every minute, stop machines left idle: Azure VMs past `auto_stop`, and
/// idle containers. Each reads its table as the first frame does, with no
/// maintenance, and only when one of its devices is saved.
pub(crate) fn install_idle_checks(cx: &mut gpui::App) {
    #[cfg(feature = "azure")]
    every(cx, std::time::Duration::from_secs(60), || {
        crate::azure::check_all_idle_devices(|| {
            crate::config::Config::load_startup()
                .ok()
                .map(|config| config.azure)
        })
    });
    #[cfg(feature = "container")]
    every(cx, crate::container::idle::CHECK, || {
        crate::container::idle::check_all(|| {
            crate::config::Config::load_startup()
                .ok()
                .map(|config| config.container)
        })
    });
    let _ = cx;
}

#[cfg(any(feature = "azure", feature = "container"))]
fn every(cx: &mut gpui::App, interval: std::time::Duration, check: impl Fn() + Send + 'static) {
    let timer = cx.background_executor().clone();
    cx.background_executor()
        .spawn(async move {
            loop {
                timer.timer(interval).await;
                check();
            }
        })
        .detach();
}
