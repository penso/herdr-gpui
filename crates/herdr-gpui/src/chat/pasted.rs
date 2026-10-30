//! A picture pasted into the composer. The program attaches files, so the
//! image is saved to a private file of its own, named by what it holds, and
//! attached as a dropped file would be.

mod error;

pub(crate) use error::Error;

use crate::{
    HerdrWindow,
    window::image_source::{PreparedImage, Source},
};
use gpui::{AppContext as _, Context, Image, Window};
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

impl HerdrWindow {
    /// Saves `image` off the UI thread, then adds the file to the next
    /// prompt of the chat over `pane_id`. Until it is saved the picture is
    /// pending, so a prompt sent meanwhile waits for it, and it joins only
    /// the conversation it was pasted into.
    pub(super) fn attach_chat_image(
        &mut self,
        pane_id: &str,
        image: Image,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        pane.uploading += 1;
        let target = pane.target.clone();
        let cursor = pane.session.since();
        let save = cx.background_spawn(async move { save(Source::Clipboard(image)) });
        let pane_id = pane_id.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let saved = save.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(pane) = this
                    .chats
                    .pane_mut(&pane_id)
                    .filter(|pane| pane.target == target)
                else {
                    return;
                };
                pane.uploading = pane.uploading.saturating_sub(1);
                if !pane.is_current(&target, cursor) {
                    // Another conversation took the pane; the picture was
                    // for the one that left.
                    this.redraw_terminal(cx);
                    return;
                }
                match saved {
                    Ok(path) => this.attach_chat_files(&pane_id, vec![path], window, cx),
                    Err(error) => {
                        pane.error = Some(format!("Image not attached: {error}"));
                        this.redraw_terminal(cx);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

/// Blocking: decodes, may transcode, and writes the file.
fn save(source: Source) -> crate::Result<PathBuf> {
    let image = source.prepare()?;
    let root = super::page::private_folder("images")
        .ok_or(Error::Folder)
        .map_err(super::Error::from)?;
    Ok(write(&root, &image).map_err(super::Error::from)?)
}

fn write(root: &Path, image: &PreparedImage) -> Result<PathBuf, Error> {
    super::page::private_dir(root).map_err(Error::Write)?;
    let mut hasher = DefaultHasher::new();
    image.bytes.hash(&mut hasher);
    let file = root.join(format!("{:016x}.{}", hasher.finish(), image.extension));
    std::fs::write(&file, &image.bytes).map_err(Error::Write)?;
    Ok(file)
}

#[cfg(test)]
mod tests;
