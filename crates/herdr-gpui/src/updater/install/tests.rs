use super::*;
// Only the macOS-only opt-in tests below use it.
#[cfg(target_os = "macos")]
use anyhow::Context as _;
use flate2::{Compression, write::GzEncoder};

mod archives;
mod guard;
mod locations;
mod replacement;
mod signing;

fn archive(root: &Path, entries: &[(&str, u8, &str)]) -> anyhow::Result<PathBuf> {
    let path = root.join("fixture.tar.gz");
    let encoder = GzEncoder::new(File::create(&path)?, Compression::fast());
    let mut archive = tar::Builder::new(encoder);
    for (path, kind, content) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o755);
        header.set_entry_type(tar::EntryType::new(*kind));
        let bytes = if *kind == b'0' {
            content.as_bytes()
        } else {
            &[]
        };
        header.set_size(bytes.len() as u64);
        if *kind == b'2' || *kind == b'1' {
            header.set_link_name(content)?;
        }
        header.set_cksum();
        archive.append_data(&mut header, path, bytes)?;
    }
    archive.into_inner()?.finish()?;
    Ok(path)
}
