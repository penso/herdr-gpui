//! Signed-in accounts' pictures. `herdr-github` hands back only the address
//! GitHub gives for one; the app reads it from the disk cache or downloads it
//! on a worker thread, never the UI thread, and keeps the images of the
//! accounts it still shows.

use super::profile_avatar;
use gpui::Image;
use std::{
    collections::HashMap,
    sync::{Arc, mpsc},
    thread,
};

/// Each shown account's picture by GitHub's avatar address. An address is
/// loaded once while it stays shown, so one that has no picture is not
/// asked for again on every poll.
#[derive(Default)]
pub(crate) struct ProfileAvatars {
    entries: HashMap<String, Entry>,
}

struct Entry {
    image: Option<Arc<Image>>,
    /// The load still running, if any.
    updates: Option<mpsc::Receiver<Arc<Image>>>,
}

impl ProfileAvatars {
    /// The picture of the account at `url`, once one was read.
    pub(crate) fn get(&self, url: &str) -> Option<Arc<Image>> {
        self.entries.get(url)?.image.clone()
    }

    /// Keeps only the pictures of `urls`, starts loading those not asked for
    /// yet, and takes what finished. Returns whether a picture changed.
    pub(crate) fn poll<'a>(&mut self, urls: impl IntoIterator<Item = &'a str>) -> bool {
        self.poll_with(urls, |url| {
            // The cached copy first, then a fresher download if it is stale.
            let (tx, rx) = mpsc::sync_channel(2);
            let _ = thread::Builder::new()
                .name("herdr-profile-avatar".into())
                .spawn(move || {
                    let (cached, updates) = profile_avatar(&url);
                    if let Some(image) = cached {
                        let _ = tx.send(image);
                    }
                    if let Some(image) = updates.and_then(|updates| updates.recv().ok()) {
                        let _ = tx.send(image);
                    }
                });
            rx
        })
    }

    pub(super) fn poll_with<'a>(
        &mut self,
        urls: impl IntoIterator<Item = &'a str>,
        mut load: impl FnMut(String) -> mpsc::Receiver<Arc<Image>>,
    ) -> bool {
        let shown: Vec<&str> = urls.into_iter().filter(|url| !url.is_empty()).collect();
        // A signed-out account's picture, and its pending load, go with it.
        let mut changed = self
            .entries
            .iter()
            .any(|(url, entry)| entry.image.is_some() && !shown.contains(&url.as_str()));
        self.entries.retain(|url, _| shown.contains(&url.as_str()));
        for url in shown {
            self.entries.entry(url.to_owned()).or_insert_with(|| Entry {
                image: None,
                updates: Some(load(url.to_owned())),
            });
        }
        for entry in self.entries.values_mut() {
            let Some(updates) = &entry.updates else {
                continue;
            };
            loop {
                match updates.try_recv() {
                    Ok(image) => {
                        entry.image = Some(image);
                        changed = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        entry.updates = None;
                        break;
                    }
                }
            }
        }
        changed
    }

    /// Shows `image` for `url` without loading it, for layout tests.
    #[cfg(test)]
    pub(crate) fn show(&mut self, url: &str, image: Arc<Image>) {
        self.entries.insert(
            url.to_owned(),
            Entry {
                image: Some(image),
                updates: None,
            },
        );
    }
}

#[cfg(test)]
mod tests;
