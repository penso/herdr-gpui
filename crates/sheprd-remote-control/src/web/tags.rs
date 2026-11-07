//! This computer's tags (`control::tags`) as the daemon holds them, and
//! `GET`/`PUT /api/v2/tags`, a phone's way to read and edit them. Home carries
//! them too, so a change reaches every phone connected.

use super::{ApiError, Device, Shared, now_ms};
use crate::{app, control::tags::ComputerTags};
use axum::{Json, extract::State};
use std::path::{Path, PathBuf};
use tokio::sync::watch;

pub(crate) struct Tags {
    file: PathBuf,
    current: watch::Sender<ComputerTags>,
}

impl Tags {
    /// The tags saved in `state_dir`. Blocking.
    pub(crate) fn load(state_dir: &Path) -> Self {
        let file = app::tags::file(state_dir);
        let current = watch::Sender::new(app::tags::load(&file));
        Self { file, current }
    }

    pub(crate) fn current(&self) -> ComputerTags {
        self.current.borrow().clone()
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<ComputerTags> {
        self.current.subscribe()
    }

    /// Offers `offer` and returns what stands. Blocking: an edit is saved
    /// before anyone hears of it, and an edit that cannot be saved is refused
    /// so its sender keeps it and tries again.
    pub(crate) fn offer(&self, offer: ComputerTags, now_ms: u64) -> app::Result<ComputerTags> {
        let stored = self.current();
        let merged = stored.clone().merged(offer, now_ms);
        if merged != stored {
            app::tags::save(&self.file, &merged)?;
            self.current.send_replace(merged.clone());
        }
        Ok(merged)
    }
}

pub(super) async fn get(State(state): State<Shared>, _device: Device) -> Json<ComputerTags> {
    Json(state.tags.current())
}

pub(super) async fn put(
    State(state): State<Shared>,
    _device: Device,
    Json(offer): Json<ComputerTags>,
) -> Result<Json<ComputerTags>, ApiError> {
    let tags = tokio::task::spawn_blocking(move || state.tags.offer(offer, now_ms()))
        .await
        .map_err(|_| ApiError::Invalid("the request was interrupted"))??;
    Ok(Json(tags))
}
