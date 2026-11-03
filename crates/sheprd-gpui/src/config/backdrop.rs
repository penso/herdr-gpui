//! `[backdrop]`: what shows behind the main window's terminals and chrome.
//! Below full opacity the theme's background turns translucent over the
//! desktop, blurred where the platform can, or over a picture. At the default
//! the window paints exactly as it does without the table.
use super::{Config, LOCAL_CONFIG, home, write_config};
use crate::backdrop::Error;
use serde::Deserialize;
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// The opacity Settings starts a picture at when the window was opaque, so
/// choosing one shows it rather than hiding it under a solid background.
pub const IMAGE_OPACITY: f32 = 0.75;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Backdrop {
    /// How solid the theme's background is, from 0 (clear) to 1 (opaque).
    pub opacity: f32,
    /// Blur what shows behind a translucent window. Ignored under a picture,
    /// which covers the desktop, and where the platform cannot blur.
    pub blur: bool,
    /// A picture behind the window, absolute or under `~/`. It covers the
    /// desktop, so the window itself stays opaque.
    pub image: Option<PathBuf>,
    /// What `blur` paints behind the window on Windows 11. Other platforms,
    /// and Windows 10, have one blur and ignore it.
    pub material: Material,
}

/// The Windows 11 system backdrop a blurred window asks DWM for.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Material {
    /// The desktop behind the window, blurred and tinted: what Windows uses
    /// for menus and flyouts.
    #[default]
    Acrylic,
    /// The wallpaper, blurred and tinted, under the whole window: what
    /// Windows uses for its own app windows. It does not show the windows
    /// behind, and is cheaper to draw than acrylic.
    Mica,
}

impl Material {
    /// The config file spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Acrylic => "acrylic",
            Self::Mica => "mica",
        }
    }
}

impl Default for Backdrop {
    fn default() -> Self {
        Self {
            opacity: 1.,
            blur: false,
            image: None,
            material: Material::default(),
        }
    }
}

impl Backdrop {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if !self.opacity.is_finite() || !(0.0..=1.).contains(&self.opacity) {
            return Err(Error::Opacity);
        }
        if let Some(image) = &self.image {
            resolve(image)?;
        }
        Ok(())
    }

    /// Whether anything shows through the theme's background. An opaque one
    /// hides a picture too, so the window keeps its usual paint.
    pub fn translucent(&self) -> bool {
        self.opacity < 1.
    }

    /// The picture to decode, with `~/` expanded.
    pub fn image_path(&self) -> Option<PathBuf> {
        self.image.as_deref().and_then(|image| resolve(image).ok())
    }
}

/// Expands a leading `~/` and requires an absolute path, which a relative
/// one would only be against whatever directory the app was started in.
fn resolve(image: &Path) -> Result<PathBuf, Error> {
    if image.as_os_str().is_empty() {
        return Err(Error::EmptyImage);
    }
    let path = match image.strip_prefix("~") {
        Ok(rest) => home().map_err(|_| Error::RelativeImage)?.join(rest),
        Err(_) => image.to_path_buf(),
    };
    if !path.is_absolute() {
        return Err(Error::RelativeImage);
    }
    Ok(path)
}

/// One `[backdrop]` key, as the Settings window saves it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Edit {
    Opacity(f32),
    Blur(bool),
    Material(Material),
    /// `None` removes the picture.
    Image(Option<PathBuf>),
}

impl Edit {
    fn entry(&self) -> Result<(&'static str, Option<toml_edit::Value>), Error> {
        let mut candidate = Backdrop::default();
        let entry = match self {
            Self::Opacity(opacity) => {
                candidate.opacity = *opacity;
                // Two decimals: the float's binary tail is noise in a file
                // people read.
                let rounded = (f64::from(*opacity) * 100.).round() / 100.;
                ("opacity", Some(rounded.into()))
            }
            Self::Blur(blur) => ("blur", Some((*blur).into())),
            Self::Material(material) => ("material", Some(material.name().into())),
            Self::Image(image) => {
                candidate.image = image.clone();
                let value = image
                    .as_deref()
                    .map(|image| image.to_str().ok_or(Error::ImagePathEncoding))
                    .transpose()?;
                ("image", value.map(Into::into))
            }
        };
        candidate.validate()?;
        Ok(entry)
    }
}

impl Config {
    pub(crate) fn save_backdrop(edit: Edit) -> crate::Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_backdrop_path(&edit, &local)
    }

    pub(super) fn save_backdrop_path(edit: &Edit, path: &Path) -> crate::Result<()> {
        let result = (|| -> crate::Result<()> {
            let (key, value) = edit.entry()?;
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            let table = document
                .entry("backdrop")
                .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                .as_table_like_mut()
                .ok_or(crate::herdr_settings::Error::Table("backdrop"))?;
            match value {
                Some(mut value) => {
                    if let Some(previous) = table.get(key).and_then(toml_edit::Item::as_value) {
                        *value.decor_mut() = previous.decor().clone();
                    }
                    table.insert(key, toml_edit::Item::Value(value));
                }
                None => {
                    table.remove(key);
                }
            }
            write_config(path, &document.to_string())
        })();
        result.map_err(|error| error.at_path(path))
    }
}
