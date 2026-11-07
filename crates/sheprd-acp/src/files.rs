//! Files attached to a prompt, turned into the content blocks the agent
//! accepts: a picture as an image, a text file as embedded text, anything
//! else (or anything too large) as a link to the file. Reads are bounded,
//! and a file that cannot be read when the prompt goes out becomes a link.

use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use acp::v1 as wire;
use base64::Engine as _;
use sheprd_chat_protocol::{Attachment, Image, ImageType, MAX_IMAGE_DATA};

/// Bytes of a text file embedded in a prompt; a larger one is linked.
const MAX_TEXT: u64 = 256 * 1024;
/// Bytes of an image read, so its base64 fits [`MAX_IMAGE_DATA`].
pub(crate) const MAX_IMAGE: u64 = (MAX_IMAGE_DATA / 4 * 3) as u64;

/// What the agent said it takes in a prompt besides text and links.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Accepts {
    pub(crate) embedded: bool,
    pub(crate) image: bool,
}

/// An attachment as it goes to the agent, with the picture the transcript
/// shows when it went as one.
pub(crate) struct Prepared {
    pub(crate) block: wire::ContentBlock,
    pub(crate) image: Option<Image>,
}

/// Whether `attachment` names a file this program can send: an absolute
/// path to a regular file.
pub(crate) fn readable(attachment: &Attachment) -> bool {
    let path = Path::new(&attachment.path);
    path.is_absolute() && path.is_file()
}

pub(crate) fn prepare(attachment: &Attachment, accepts: Accepts) -> Prepared {
    let path = PathBuf::from(&attachment.path);
    if accepts.image
        && let Some(kind) = image_type(&path)
        && let Some(bytes) = read(&path, MAX_IMAGE)
        && let Some(image) = Image::new(
            kind.mime(),
            base64::engine::general_purpose::STANDARD.encode(bytes),
        )
    {
        let block =
            wire::ContentBlock::Image(wire::ImageContent::new(image.data.clone(), kind.mime()));
        return Prepared {
            block,
            image: Some(image),
        };
    }
    let uri = uri(&path);
    if accepts.embedded
        && let Some(text) = read(&path, MAX_TEXT)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .filter(|text| !text.contains('\0'))
    {
        let contents = wire::TextResourceContents::new(text, uri);
        return Prepared {
            block: wire::ContentBlock::Resource(wire::EmbeddedResource::new(
                wire::EmbeddedResourceResource::TextResourceContents(contents),
            )),
            image: None,
        };
    }
    Prepared {
        block: wire::ContentBlock::ResourceLink(wire::ResourceLink::new(attachment.name(), uri)),
        image: None,
    }
}

pub(crate) fn image_type(path: &Path) -> Option<ImageType> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    ImageType::from_mime(&format!("image/{extension}"))
}

/// The whole file, when it is at most `limit` bytes.
pub(crate) fn read(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

fn uri(path: &Path) -> String {
    url::Url::from_file_path(path)
        .map_or_else(|()| format!("file://{}", path.display()), String::from)
}

#[cfg(test)]
mod tests;
