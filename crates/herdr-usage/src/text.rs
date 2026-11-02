//! Words a report shows: a section's title, a fact's label and value, a
//! share's label, a balance's name, a window the service names, and the
//! account line. Crates below the app have no messages (docs/i18n.md), so a
//! provider names a message by ID, in the app's `usage-report.ftl` for words
//! several providers share or its own `usage-<id>.ftl`, and the app writes
//! it in the shown language. What the service says itself, such as a model,
//! a project, or a plan, and data such as dates and amounts stay as they are.
//!
//! The app checks every ID a provider names, and the arguments it chains
//! with [`Text::arg`], against the English messages; so a message's
//! arguments are always chained where its ID is named.

use std::{cmp::Ordering, fmt};

#[derive(Clone, Debug)]
pub enum Text {
    /// A message by ID, such as `usage-report-credits-left`, with the
    /// arguments it takes.
    Message {
        id: &'static str,
        args: Vec<(&'static str, Arg)>,
    },
    /// The service's own words, or data such as a date or an amount.
    Service(String),
}

/// A message's argument: a number its selectors may pick a plural by, or
/// words of either kind.
#[derive(Clone, Debug)]
pub enum Arg {
    Number(f64),
    Text(Text),
}

impl Text {
    pub const fn message(id: &'static str) -> Self {
        Self::Message {
            id,
            args: Vec::new(),
        }
    }

    pub fn service(text: impl Into<String>) -> Self {
        Self::Service(text.into())
    }

    /// Adds an argument to a message; the service's words take none.
    #[must_use]
    pub fn arg(mut self, name: &'static str, value: impl Into<Arg>) -> Self {
        if let Self::Message { args, .. } = &mut self {
            args.push((name, value.into()));
        }
        self
    }

    /// The service's words, or the message's ID, for logs and tests: the
    /// shown text is the app's to write.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Message { id, .. } => id,
            Self::Service(text) => text,
        }
    }

    /// The message's ID, or `None` for the service's words.
    pub fn id(&self) -> Option<&'static str> {
        match self {
            Self::Message { id, .. } => Some(id),
            Self::Service(_) => None,
        }
    }
}

/// The ID, or the service's words, for logs and tests: the shown text is
/// the app's to write.
impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

macro_rules! number {
    ($($type:ty),*) => {$(
        impl From<$type> for Arg {
            fn from(value: $type) -> Self {
                Self::Number(value as f64)
            }
        }
    )*};
}

number!(f32, f64, i32, i64, u32, u64, usize);

impl From<Text> for Arg {
    fn from(text: Text) -> Self {
        Self::Text(text)
    }
}

impl From<String> for Arg {
    fn from(text: String) -> Self {
        Self::Text(Text::Service(text))
    }
}

impl From<&str> for Arg {
    fn from(text: &str) -> Self {
        Self::Text(Text::service(text))
    }
}

/// Tests and logs compare words with the service's text or a message's ID.
impl PartialEq<&str> for Text {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<str> for Text {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

// Windows sort by kind, a named one by its name, so words order and compare
// as wholes; numbers by `total_cmp`, so the order is total.
impl Ord for Text {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (
                Self::Message { id, args },
                Self::Message {
                    id: other_id,
                    args: other_args,
                },
            ) => id.cmp(other_id).then_with(|| args.cmp(other_args)),
            (Self::Service(text), Self::Service(other)) => text.cmp(other),
            (Self::Message { .. }, Self::Service(_)) => Ordering::Less,
            (Self::Service(_), Self::Message { .. }) => Ordering::Greater,
        }
    }
}

impl PartialOrd for Text {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Text {}

impl Ord for Arg {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Number(number), Self::Number(other)) => number.total_cmp(other),
            (Self::Text(text), Self::Text(other)) => text.cmp(other),
            (Self::Number(_), Self::Text(_)) => Ordering::Less,
            (Self::Text(_), Self::Number(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for Arg {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Arg {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Arg {}
