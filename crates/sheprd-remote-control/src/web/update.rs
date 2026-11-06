//! Updating Sheprd from a phone (`control::app_update`): `GET /api/v2/update`
//! says where the app's updater stands, and `POST` asks it to update. The
//! app takes the request with its next poll, so a request needs a running
//! app; a daemon left serving after the app quit refuses it.

mod error;

use super::{ApiError, Device, Shared};
use crate::control::app_update::AppUpdate;
use axum::{Json, extract::State};
pub(crate) use error::Error;
use serde::Serialize;
use std::time::{Duration, Instant};

/// How long after its last report the app counts as closed. It reports
/// every few seconds while a window is open.
const APP_SILENCE: Duration = Duration::from_secs(15);

/// One report from a run of the app.
#[derive(Debug)]
struct Report {
    at: Instant,
    instance: u32,
    update: AppUpdate,
}

/// What the app last said, and which run of it a phone is waiting for.
#[derive(Debug, Default)]
pub(crate) struct Relay {
    asked: Option<u32>,
    report: Option<Report>,
}

impl Relay {
    /// Keeps the app's report, and hands it a phone's request. A request
    /// goes only to the run it was made to, while that run still reports: it
    /// must not update, unseen, an app opened later, however soon.
    pub(crate) fn report(&mut self, instance: u32, update: AppUpdate, now: Instant) -> bool {
        let reporting = self.app(now).is_some();
        self.report = Some(Report {
            at: now,
            instance,
            update,
        });
        self.asked.take() == Some(instance) && reporting
    }

    fn live(&self, now: Instant) -> Option<&Report> {
        self.report
            .as_ref()
            .filter(|report| now.saturating_duration_since(report.at) < APP_SILENCE)
    }

    /// The app's report, while it still reports.
    fn app(&self, now: Instant) -> Option<&AppUpdate> {
        self.live(now).map(|report| &report.update)
    }

    /// Notes a phone's request for the app's next report.
    fn ask(&mut self, now: Instant) -> Result<(), Error> {
        let report = self.live(now).ok_or(Error::AppClosed)?;
        self.asked = Some(report.instance);
        Ok(())
    }

    fn status(&self, now: Instant) -> Status {
        let live = self.live(now);
        Status {
            app: live.map(|report| report.update.clone()),
            asked: live.is_some_and(|report| self.asked == Some(report.instance)),
        }
    }
}

/// What a phone reads.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub(super) struct Status {
    /// The updater, or `None` while Sheprd is not open on this computer.
    app: Option<AppUpdate>,
    /// A request is waiting for the app's next poll.
    asked: bool,
}

/// `GET /api/v2/update`.
pub(super) async fn status(State(state): State<Shared>, _device: Device) -> Json<Status> {
    Json(state.updates().status(Instant::now()))
}

/// `POST /api/v2/update`.
pub(super) async fn request(
    State(state): State<Shared>,
    _device: Device,
) -> Result<Json<Status>, ApiError> {
    let now = Instant::now();
    let mut updates = state.updates();
    updates.ask(now)?;
    Ok(Json(updates.status(now)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
