//! Saving the notes' screenshots where the agent can read them.

use gpui::Image;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

/// Screenshots older than this are removed when the next ones are saved.
pub(super) const SCREENSHOT_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Writes the notes' screenshots where the agent can read them, returning
/// each note's file. They live in the app's private state folder, and ones
/// older than a week are removed first. Blocking; run off the UI thread.
pub(super) fn save_screenshots(
    images: &[Option<Arc<Image>>],
) -> crate::Result<Vec<Option<PathBuf>>> {
    let dir = crate::preferences::state_dir()
        .ok_or(crate::Error::MissingStateRoot)?
        .join("annotations");
    save_screenshots_in(&dir, images, SystemTime::now())
}

pub(super) fn save_screenshots_in(
    dir: &std::path::Path,
    images: &[Option<Arc<Image>>],
    now: SystemTime,
) -> crate::Result<Vec<Option<PathBuf>>> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > SCREENSHOT_AGE);
            if old {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let stamp = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default();
    images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let Some(image) = image else {
                return Ok(None);
            };
            let path = dir.join(format!("note-{stamp}-{}.png", index + 1));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            use std::io::Write as _;
            options.open(&path)?.write_all(&image.bytes)?;
            Ok(Some(path))
        })
        .collect()
}
