//! Public images only. All filesystem access and decoding happens on workers.
#![forbid(unsafe_code)]

use gpui_kit::{Image, ImageFormat};
#[cfg(unix)]
use rustix::fs::{AtFlags, FlockOperation, Mode, OFlags, flock, open, openat, renameat, unlinkat};
#[cfg(unix)]
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::{
    fs::{DirBuilder, File},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt},
    time::{Duration, UNIX_EPOCH},
};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

pub(super) const LIMIT: usize = 1_000_000;
#[cfg(unix)]
const TTL: Duration = Duration::from_secs(24 * 60 * 60);
#[cfg(unix)]
const SLOTS: u8 = 128;

#[cfg(unix)]
pub(super) fn root() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")))?;
    base.is_absolute()
        .then(|| base.join("herdr-gpui/avatars-v1"))
}

/// The disk cache relies on `openat`, `flock`, and POSIX ownership and mode
/// checks to keep a shared cache directory safe. Off POSIX there is no root, so
/// avatars stay in memory for the life of the process and the two accessors
/// below are unreachable.
#[cfg(not(unix))]
pub(super) fn root() -> Option<PathBuf> {
    None
}

#[cfg(not(unix))]
pub(super) fn read(_path: &Path, _url: &str, _now: SystemTime) -> Option<(Arc<Image>, bool)> {
    None
}

#[cfg(not(unix))]
pub(super) fn write(_path: &Path, _url: &str, _bytes: &[u8], _now: SystemTime) -> Option<()> {
    None
}

pub(super) fn decode(bytes: &[u8]) -> Option<Arc<Image>> {
    if bytes.is_empty() || bytes.len() > LIMIT {
        return None;
    }
    let format = image::guess_format(bytes).ok()?;
    let gpui_format = match format {
        image::ImageFormat::Png => ImageFormat::Png,
        image::ImageFormat::Jpeg => ImageFormat::Jpeg,
        image::ImageFormat::Gif => ImageFormat::Gif,
        image::ImageFormat::WebP => ImageFormat::Webp,
        _ => return None,
    };
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(512);
    limits.max_image_height = Some(512);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().ok()?;
    if format == image::ImageFormat::Gif {
        use image::{AnimationDecoder, ImageDecoder};
        let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(512);
        limits.max_image_height = Some(512);
        limits.max_alloc = Some(4 * 1024 * 1024);
        decoder.set_limits(limits).ok()?;
        let mut frames = decoder.into_frames();
        frames.next()?.ok()?;
        // GPUI retains every GIF frame. Avatars must not expand into an
        // unbounded animation even when their encoded file is small.
        if frames.next().is_some() {
            return None;
        }
    }
    Some(Arc::new(Image::from_bytes(gpui_format, bytes.to_vec())))
}

#[cfg(unix)]
struct Directory {
    dir: File,
    lock: File,
}

#[cfg(unix)]
impl Drop for Directory {
    fn drop(&mut self) {
        // A concurrently spawning child can briefly inherit the open description
        // before CLOEXEC takes effect. Release explicitly rather than relying on
        // the last descriptor closing in that child.
        let _ = flock(&self.lock, FlockOperation::Unlock);
    }
}

#[cfg(unix)]
impl Directory {
    fn open(path: &Path) -> Option<Self> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .ok()?;
        let dir = File::from(
            open(
                path,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .ok()?,
        );
        let meta = dir.metadata().ok()?;
        if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o777 != 0o700 {
            return None;
        }
        let lock = File::from(
            openat(
                &dir,
                "lock",
                OFlags::RDWR
                    | OFlags::CREATE
                    | OFlags::NOFOLLOW
                    | OFlags::NONBLOCK
                    | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )
            .ok()?,
        );
        if !private_file(&lock) {
            return None;
        }
        // Never wait for another process; a contended cache is just a miss.
        flock(&lock, FlockOperation::NonBlockingLockExclusive).ok()?;
        Some(Self { dir, lock })
    }
}

#[cfg(unix)]
fn private_file(file: &File) -> bool {
    file.metadata().is_ok_and(|m| {
        m.is_file()
            && m.uid() == rustix::process::geteuid().as_raw()
            && m.mode() & 0o777 == 0o600
            && m.nlink() == 1
    })
}

#[cfg(unix)]
fn key(url: &str) -> ([u8; 32], String) {
    let hash: [u8; 32] = Sha256::digest(url.as_bytes()).into();
    // Fixed slots bound disk usage without walking or deleting arbitrary paths.
    (hash, format!("{:02x}.avatar", hash[0] % SLOTS))
}

#[cfg(unix)]
pub(super) fn read(path: &Path, url: &str, now: SystemTime) -> Option<(Arc<Image>, bool)> {
    let directory = Directory::open(path)?;
    let (hash, name) = key(url);
    let file = File::from(
        openat(
            &directory.dir,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .ok()?,
    );
    if !private_file(&file) {
        return None;
    }
    let result = (|| {
        let mut bytes = Vec::new();
        file.take((LIMIT + 41) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > LIMIT + 40 || bytes.len() < 40 {
            return None;
        }
        let image = decode(&bytes[40..])?;
        let timestamp = u64::from_le_bytes(bytes[32..40].try_into().ok()?);
        let age = now
            .duration_since(UNIX_EPOCH.checked_add(Duration::from_secs(timestamp))?)
            .ok();
        Some((bytes[..32] == hash, image, age.is_some_and(|age| age < TTL)))
    })();
    match result {
        Some((true, image, fresh)) => Some((image, fresh)),
        Some((false, _, _)) => None,
        None => {
            let _ = unlinkat(&directory.dir, name.as_str(), AtFlags::empty());
            None
        }
    }
}

#[cfg(unix)]
pub(super) fn write(path: &Path, url: &str, bytes: &[u8], now: SystemTime) -> Option<()> {
    decode(bytes)?;
    let directory = Directory::open(path)?;
    let (hash, name) = key(url);
    let timestamp = now.duration_since(UNIX_EPOCH).ok()?.as_secs();
    // One temporary entry, recovered under the lock after an interrupted write.
    let _ = unlinkat(&directory.dir, "pending", AtFlags::empty());
    let mut file = File::from(
        openat(
            &directory.dir,
            "pending",
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .ok()?,
    );
    let result = (|| {
        file.write_all(&hash).ok()?;
        file.write_all(&timestamp.to_le_bytes()).ok()?;
        file.write_all(bytes).ok()?;
        file.sync_all().ok()?;
        renameat(&directory.dir, "pending", &directory.dir, name.as_str()).ok()?;
        Some(())
    })();
    let _ = unlinkat(&directory.dir, "pending", AtFlags::empty());
    result
}

// The fixtures build a POSIX cache directory with modes and hard links.
#[cfg(all(test, unix))]
pub(super) mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    pub(crate) struct Fixture(pub PathBuf);
    impl Fixture {
        pub(crate) fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            Self(std::env::temp_dir().join(format!(
                "herdr-avatar-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    pub(crate) fn png() -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn lock_contention_is_nonblocking_and_release_survives_inherited_descriptors() {
        let fixture = Fixture::new();
        let directory = Directory::open(&fixture.0).unwrap();
        assert!(Directory::open(&fixture.0).is_none());
        let inherited = directory.lock.try_clone().unwrap();
        drop(directory);
        assert!(Directory::open(&fixture.0).is_some());
        drop(inherited);
    }

    #[test]
    fn persistent_roundtrip_ttl_and_collision_isolation() {
        let fixture = Fixture::new();
        let now = UNIX_EPOCH + Duration::from_secs(100_000);
        let url = "https://avatars.githubusercontent.com/u/1";
        let bytes = png();
        write(&fixture.0, url, &bytes, now).unwrap();
        assert_eq!(
            read(&fixture.0, url, now).unwrap().0.bytes.as_slice(),
            bytes.as_slice()
        );
        assert!(
            read(&fixture.0, url, now + TTL - Duration::from_secs(1))
                .unwrap()
                .1
        );
        assert!(!read(&fixture.0, url, now + TTL).unwrap().1);
        assert!(
            !read(&fixture.0, url, now - Duration::from_secs(1))
                .unwrap()
                .1
        );
        let collision = (2..10_000)
            .map(|n| format!("https://avatars.githubusercontent.com/u/{n}"))
            .find(|other| key(other).1 == key(url).1)
            .unwrap();
        assert!(read(&fixture.0, &collision, now).is_none());
        write(&fixture.0, &collision, &bytes, now).unwrap();
        assert!(read(&fixture.0, url, now).is_none());
        assert!(read(&fixture.0, &collision, now).is_some());
    }

    #[test]
    fn corrupt_oversized_and_unsafe_entries_do_not_escape_cache() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let fixture = Fixture::new();
        let now = SystemTime::now();
        let url = "https://avatars.githubusercontent.com/u/1";
        write(&fixture.0, url, &png(), now).unwrap();
        let slot = fixture.0.join(key(url).1);
        for bytes in [b"corrupt".to_vec(), vec![0; LIMIT + 41]] {
            std::fs::write(&slot, bytes).unwrap();
            assert!(read(&fixture.0, url, now).is_none());
            assert!(!slot.exists());
            write(&fixture.0, url, &png(), now).unwrap();
        }
        let outside = fixture.0.join("untouched");
        std::fs::write(&outside, b"untouched").unwrap();
        std::fs::remove_file(&slot).unwrap();
        symlink(&outside, &slot).unwrap();
        assert!(read(&fixture.0, url, now).is_none());
        write(&fixture.0, url, &png(), now).unwrap();
        assert_eq!(std::fs::read(&outside).unwrap(), b"untouched");
        assert_eq!(std::fs::metadata(&slot).unwrap().mode() & 0o777, 0o600);
        std::fs::hard_link(&slot, fixture.0.join("hardlink")).unwrap();
        assert!(read(&fixture.0, url, now).is_none());
        let linked = fixture.0.join("linked");
        symlink(&fixture.0, &linked).unwrap();
        assert!(read(&linked, url, now).is_none());
        assert!(write(&linked, url, &png(), now).is_none());
        std::fs::set_permissions(&fixture.0, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(read(&fixture.0, url, now).is_none());
        assert!(write(&fixture.0, url, &png(), now).is_none());
    }

    #[test]
    fn quota_and_decode_limits() {
        let fixture = Fixture::new();
        let now = SystemTime::now();
        let bytes = png();
        for n in 0..300 {
            write(
                &fixture.0,
                &format!("https://avatars.githubusercontent.com/u/{n}"),
                &bytes,
                now,
            )
            .unwrap();
        }
        let entries: Vec<_> = std::fs::read_dir(&fixture.0)
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(entries.len() <= usize::from(SLOTS) + 1);
        assert!(
            entries
                .iter()
                .map(|e| e.metadata().unwrap().len())
                .sum::<u64>()
                <= 128 * (LIMIT as u64 + 40)
        );
        assert!(decode(&vec![0; LIMIT + 1]).is_none());
        assert!(decode(b"not an image").is_none());
        let mut huge = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(513, 1)
            .write_to(&mut huge, image::ImageFormat::Png)
            .unwrap();
        assert!(decode(huge.get_ref()).is_none());
        let mut animated = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut animated);
            for _ in 0..2 {
                encoder
                    .encode_frame(image::Frame::new(image::RgbaImage::new(2, 2)))
                    .unwrap();
            }
        }
        assert!(decode(&animated).is_none());
        assert!(write(&fixture.0, "invalid", huge.get_ref(), now).is_none());
        assert!(
            key("../../outside")
                .1
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || b".avatar".contains(&b))
        );
    }
}
