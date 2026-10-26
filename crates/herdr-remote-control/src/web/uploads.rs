//! Pictures a phone attaches to a prompt. The chat program reads
//! attachments by path on this computer, which a phone must never name, so
//! an upload is checked to be an image by its bytes, written 0600 to a
//! private folder under a random name, and known to the phone only by an
//! opaque id that a prompt's attachment carries. Uploads are forgotten and
//! deleted after a day, and only a bounded number are kept.

use herdr_chat_protocol::{Attachment, ImageType};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

/// The largest picture a phone may send; the chat program's own image
/// limit is 5 MiB of base64, which this stays under.
pub(crate) const MAX_UPLOAD: usize = 3_500_000;
/// Uploads kept at once.
const MAX_UPLOADS: usize = 64;
/// How long an upload stays attachable.
const LIFETIME_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    // keep-sorted start
    #[error("that is not a PNG, JPEG, GIF, or WebP picture")]
    NotAnImage,
    #[error("the picture is larger than {MAX_UPLOAD} bytes")]
    TooLarge,
    #[error("that attachment is no longer available; attach it again")]
    UnknownUpload,
    #[error("could not store the picture")]
    Write(#[source] std::io::Error),
    // keep-sorted end
}

struct Upload {
    path: PathBuf,
    created_ms: u64,
}

pub(crate) struct Uploads {
    dir: PathBuf,
    uploads: Mutex<HashMap<String, Upload>>,
}

/// The kind of picture `bytes` hold, by their signature.
pub(crate) fn sniff(bytes: &[u8]) -> Option<ImageType> {
    match bytes {
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => Some(ImageType::Png),
        [0xff, 0xd8, 0xff, ..] => Some(ImageType::Jpeg),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some(ImageType::Gif),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some(ImageType::Webp),
        _ => None,
    }
}

fn extension(kind: ImageType) -> &'static str {
    match kind {
        ImageType::Gif => "gif",
        ImageType::Jpeg => "jpg",
        ImageType::Png => "png",
        ImageType::Webp => "webp",
    }
}

impl Uploads {
    pub(crate) fn new(state_dir: &Path) -> Self {
        Self {
            dir: state_dir.join("uploads"),
            uploads: Mutex::new(HashMap::new()),
        }
    }

    /// Stores `bytes` and returns the id a prompt attaches. Blocking.
    pub(crate) fn store(&self, id: String, bytes: &[u8], now_ms: u64) -> Result<String, Error> {
        if bytes.len() > MAX_UPLOAD {
            return Err(Error::TooLarge);
        }
        let kind = sniff(bytes).ok_or(Error::NotAnImage)?;
        self.prune(now_ms);
        private_dir(&self.dir).map_err(Error::Write)?;
        let path = self.dir.join(format!("{id}.{}", extension(kind)));
        write_private(&path, bytes).map_err(Error::Write)?;
        let mut uploads = self.uploads.lock().unwrap_or_else(PoisonError::into_inner);
        if uploads.len() >= MAX_UPLOADS
            && let Some(oldest) = uploads
                .iter()
                .min_by_key(|(_, upload)| upload.created_ms)
                .map(|(id, _)| id.clone())
            && let Some(upload) = uploads.remove(&oldest)
        {
            let _ = std::fs::remove_file(upload.path);
        }
        uploads.insert(
            id.clone(),
            Upload {
                path,
                created_ms: now_ms,
            },
        );
        Ok(id)
    }

    /// The attachments a phone named by id, as the paths the chat program
    /// reads. An upload past its lifetime is gone even before a sweep.
    pub(crate) fn resolve(
        &self,
        attachments: &[Attachment],
        now_ms: u64,
    ) -> Result<Vec<Attachment>, Error> {
        attachments
            .iter()
            .map(|attachment| {
                self.path(&attachment.path, now_ms).map(|path| Attachment {
                    path: path.to_string_lossy().into_owned(),
                })
            })
            .collect()
    }

    /// Where the upload `id` names was written, while it is still kept.
    pub(crate) fn path(&self, id: &str, now_ms: u64) -> Result<PathBuf, Error> {
        let uploads = self.uploads.lock().unwrap_or_else(PoisonError::into_inner);
        uploads
            .get(id)
            .filter(|upload| now_ms.saturating_sub(upload.created_ms) < LIFETIME_MS)
            .map(|upload| upload.path.clone())
            .ok_or(Error::UnknownUpload)
    }

    /// Deletes every picture an earlier run left: their ids died with it,
    /// so nothing can attach them. Blocking; run before serving.
    pub(crate) fn remove_leftovers(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_file()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// Forgets and deletes uploads older than a day. Blocking.
    pub(crate) fn prune(&self, now_ms: u64) {
        let mut uploads = self.uploads.lock().unwrap_or_else(PoisonError::into_inner);
        uploads.retain(|_, upload| {
            let keep = now_ms.saturating_sub(upload.created_ms) < LIFETIME_MS;
            if !keep {
                let _ = std::fs::remove_file(&upload.path);
            }
            keep
        });
    }
}

fn private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
