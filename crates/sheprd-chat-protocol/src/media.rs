//! Pictures in the conversation. The program checks an image's type and size
//! before it enters the transcript; a client still treats the bytes as
//! untrusted and decodes them with the same bounds.

use serde::{Deserialize, Serialize};

/// The most base64 text one image carries, about 3.75 MiB of image bytes.
pub const MAX_IMAGE_DATA: usize = 5 * 1024 * 1024;

/// Who put a picture in the conversation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Author {
    User,
    Agent,
}

/// The raster formats every client decodes. Vector formats are left out:
/// an SVG may name other files and fonts to load.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ImageType {
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/webp")]
    Webp,
}

impl ImageType {
    /// The type a MIME type names, ignoring case and parameters.
    pub fn from_mime(mime: &str) -> Option<Self> {
        let essence = mime.split(';').next().unwrap_or("").trim();
        [Self::Gif, Self::Jpeg, Self::Png, Self::Webp]
            .into_iter()
            .find(|kind| kind.mime().eq_ignore_ascii_case(essence))
            .or_else(|| {
                essence
                    .eq_ignore_ascii_case("image/jpg")
                    .then_some(Self::Jpeg)
            })
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Gif => "image/gif",
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }
}

/// An image as standard base64 text of at most [`MAX_IMAGE_DATA`] bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Image {
    pub data: String,
    #[serde(rename = "mime_type")]
    pub kind: ImageType,
}

impl Image {
    /// The image, when its type is one clients decode and its data is
    /// bounded base64 text; whether the bytes decode is left to the client.
    pub fn new(mime: &str, data: String) -> Option<Self> {
        let kind = ImageType::from_mime(mime)?;
        let valid = !data.is_empty()
            && data.len() <= MAX_IMAGE_DATA
            && data
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='));
        valid.then_some(Self { data, kind })
    }
}
