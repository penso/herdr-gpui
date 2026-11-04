//! Addresses an explicit hyperlink names that are neither a web page nor a
//! local file, such as `mailto:`. The system opens them in whatever
//! application registered the scheme, so only the schemes listed here open
//! without the user confirming the address first.

use std::fmt;

/// The longest address accepted, the bound web addresses share.
const MAX_URL_BYTES: usize = 8192;

/// Schemes that open without a confirmation.
const TRUSTED: [&str; 2] = ["mailto", "vscode"];

/// A bounded, parsed address with a scheme other than `http`, `https`, and
/// `file`, which have their own handling. If a value exists, it is safe to
/// show and to hand to the system's opener as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExternalUrl(url::Url);

impl ExternalUrl {
    /// Whether the address opens without asking the user first.
    pub(crate) fn trusted(&self) -> bool {
        TRUSTED.contains(&self.0.scheme())
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<&str> for ExternalUrl {
    type Error = crate::Error;

    fn try_from(value: &str) -> crate::Result<Self> {
        if value.len() > MAX_URL_BYTES || value.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(crate::Error::InvalidExternalUrl);
        }
        let url = url::Url::parse(value).map_err(|_| crate::Error::InvalidExternalUrl)?;
        // Serialization can lengthen an address, so the bound applies to
        // what is shown and opened too.
        if matches!(url.scheme(), "http" | "https" | "file") || url.as_str().len() > MAX_URL_BYTES {
            return Err(crate::Error::InvalidExternalUrl);
        }
        Ok(Self(url))
    }
}

impl fmt::Display for ExternalUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
