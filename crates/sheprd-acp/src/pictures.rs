//! Pictures the agent points at on this machine, read so every client can
//! draw them: the local files an agent message links as Markdown images,
//! and the image files a finished tool call wrote or read. This program
//! runs beside the agent, so it reads what the agent sees, on an SSH host
//! too. Reads are bounded, and a file only counts as a picture when its
//! first bytes say so, whatever its name; anything else is left as text.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use sheprd_chat_protocol::{Image, ImageType, Tool, ToolContent, ToolStatus};

/// Pictures read for one message or tool call; later ones stay text.
pub(crate) const MAX_PICTURES: usize = 4;

/// The picture at `path`, when the file is a bounded image of a type
/// clients decode and its bytes match that type.
pub(crate) fn read(path: &Path) -> Option<Image> {
    let kind = crate::files::image_type(path)?;
    let bytes = crate::files::read(path, crate::files::MAX_IMAGE)?;
    if sniff(&bytes)? != kind {
        return None;
    }
    Image::new(
        kind.mime(),
        base64::engine::general_purpose::STANDARD.encode(bytes),
    )
}

/// The image type a file's first bytes declare.
fn sniff(bytes: &[u8]) -> Option<ImageType> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageType::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(ImageType::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(ImageType::Gif)
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some(ImageType::Webp)
    } else {
        None
    }
}

/// The local files `text` shows as Markdown images, `![alt](target)`, in
/// order and once each: an absolute path, a `file:` URL, or a path relative
/// to `cwd`. Web addresses are the client's to load when the user asks.
pub(crate) fn linked(text: &str, cwd: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("![") {
        rest = &rest[start + 2..];
        let Some(close) = rest.find("](") else {
            break;
        };
        if rest[..close].contains('\n') {
            continue;
        }
        rest = &rest[close + 2..];
        let Some(end) = rest.find(')') else {
            break;
        };
        let target = rest[..end].trim();
        rest = &rest[end + 1..];
        // A title may follow the target: `![a](path "title")`.
        let target = target
            .strip_prefix('<')
            .and_then(|inner| inner.split_once('>'))
            .map_or_else(
                || target.split_whitespace().next().unwrap_or(""),
                |(inner, _)| inner,
            );
        if let Some(path) = local(target, cwd)
            && !paths.contains(&path)
        {
            paths.push(path);
            if paths.len() == MAX_PICTURES {
                break;
            }
        }
    }
    paths
}

fn local(target: &str, cwd: &Path) -> Option<PathBuf> {
    if target.is_empty() || target.contains('\n') {
        return None;
    }
    if target.starts_with("file:") {
        let url = url::Url::parse(target).ok()?;
        // A host other than this machine names a network share.
        if url
            .host_str()
            .is_some_and(|host| !host.is_empty() && host != "localhost")
        {
            return None;
        }
        return url.to_file_path().ok().filter(|path| !network(path));
    }
    // Opening a share's path makes Windows sign in to that server with the
    // user's credentials, so a link never names one.
    if target.starts_with("\\\\") || target.starts_with("//") {
        return None;
    }
    // Any other scheme, such as https: or data:, is not a file here. A
    // Windows drive letter looks like a one-letter scheme, so it is kept.
    if let Some((scheme, _)) = target.split_once(':')
        && scheme.len() > 1
        && scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "+-.".contains(ch))
    {
        return None;
    }
    let path = Path::new(target);
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        cwd.join(path)
    };
    (!network(&path)).then_some(path)
}

/// Whether `path` names a network share or a device rather than a file on
/// this machine's own disks.
fn network(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    matches!(
        path.components().next(),
        Some(Component::Prefix(prefix)) if matches!(
            prefix.kind(),
            Prefix::UNC(..) | Prefix::VerbatimUNC(..) | Prefix::DeviceNS(..) | Prefix::Verbatim(..)
        )
    ) || path.as_os_str().to_string_lossy().starts_with("//")
}

/// The pictures a finished tool call wrote or read, from the files it
/// names, when it shows none of its own.
pub(crate) fn of_tool(tool: &Tool, cwd: &Path) -> Vec<ToolContent> {
    if tool.status != ToolStatus::Completed
        || tool
            .content
            .iter()
            .any(|content| matches!(content, ToolContent::Image { .. }))
    {
        return Vec::new();
    }
    let diffs = tool.content.iter().filter_map(|content| match content {
        ToolContent::Diff { path, .. } => Some(path.as_str()),
        _ => None,
    });
    let mut seen: Vec<PathBuf> = Vec::new();
    tool.locations
        .iter()
        .map(|location| without_line(location))
        .chain(diffs)
        .filter_map(|name| local(name, cwd))
        .filter(|path| crate::files::image_type(path).is_some())
        .filter(|path| {
            let new = !seen.contains(path);
            seen.push(path.clone());
            new
        })
        .take(MAX_PICTURES)
        .filter_map(|path| read(&path))
        .map(|image| ToolContent::Image { image })
        .collect()
}

/// A location without the `:line` a tool call may add to it.
fn without_line(location: &str) -> &str {
    match location.rsplit_once(':') {
        Some((path, line))
            if !line.is_empty() && line.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            path
        }
        _ => location,
    }
}

#[cfg(test)]
mod tests;
