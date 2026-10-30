//! The languages Herdr UI can show (docs/i18n.md): locales and their text
//! direction, the `language` setting and its negotiation against the
//! system's languages with an `en-US` fallback, the Fluent catalog of
//! embedded messages, the `en-XA`/`en-XB` pseudo-locales, and which regional
//! Han glyphs a Chinese, Japanese, or Korean language wants. No GPUI; the
//! app embeds its `.ftl` files, keeps the current catalog, and redraws.

mod catalog;
mod cjk;
mod error;
mod locale;
mod negotiate;
pub mod pseudo;

pub use catalog::{Catalog, File};
pub use cjk::CjkScript;
pub use error::{Error, Result};
pub use fluent::{FluentArgs, FluentValue};
pub use locale::{Direction, Language, Locale, Pseudo};
pub use negotiate::{negotiate, system_languages};
pub use unic_langid::LanguageIdentifier;

#[cfg(test)]
mod tests;
