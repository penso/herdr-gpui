//! `option_as_alt`: whether macOS Option reaches a pane as Alt.
use serde::Deserialize;

/// Whether macOS Option sends Alt shortcuts to a pane or types the character
/// the keyboard layout puts on it. Other platforms have no Option layer, so
/// Alt always reaches the pane there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptionAsAlt {
    /// The left Option sends Alt on the U.S. and ABC layouts, whose Option
    /// layer only holds symbols like `π` and dead keys like `´`; the right
    /// Option still types them. Both type elsewhere, where Option holds `@`,
    /// `[`, or letters.
    #[default]
    Auto,
    Always,
    Never,
    /// Only the left Option sends Alt; the right one types.
    Left,
    /// Only the right Option sends Alt; the left one types.
    Right,
}

/// Which Option keys a keystroke was made with, as macOS reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OptionKeys {
    pub left: bool,
    pub right: bool,
}

impl OptionKeys {
    /// What a keystroke whose side is unknown counts as: the left key,
    /// which is where Alt has always been.
    pub const LEFT: Self = Self {
        left: true,
        right: false,
    };

    /// The side from an `NSEvent`'s modifier flags, whose device-dependent
    /// bits tell the two keys apart. Flags naming neither count as the left.
    #[cfg(any(target_os = "macos", test))]
    pub fn from_device_flags(flags: usize) -> Self {
        const LEFT: usize = 0x20;
        const RIGHT: usize = 0x40;
        let keys = Self {
            left: flags & LEFT != 0,
            right: flags & RIGHT != 0,
        };
        if keys.left || keys.right {
            keys
        } else {
            Self::LEFT
        }
    }
}

impl OptionAsAlt {
    /// macOS layouts whose Option characters a terminal user rarely types.
    const ALT_LAYOUTS: [&'static str; 2] = ["com.apple.keylayout.US", "com.apple.keylayout.ABC"];

    /// Whether Option-modified keys made with `keys` go to the pane as Alt
    /// under `layout`, the platform keyboard layout ID.
    pub fn sends_alt(self, layout: &str, keys: OptionKeys) -> bool {
        if !cfg!(target_os = "macos") {
            return true;
        }
        match self {
            Self::Auto => Self::ALT_LAYOUTS.contains(&layout) && keys.left,
            Self::Always => true,
            Self::Never => false,
            Self::Left => keys.left,
            Self::Right => keys.right,
        }
    }
}

impl<'de> Deserialize<'de> for OptionAsAlt {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Value {
            Bool(bool),
            Name(String),
        }
        const NAMES: &[&str] = &["auto", "left", "right"];
        match Value::deserialize(deserializer)? {
            Value::Bool(true) => Ok(Self::Always),
            Value::Bool(false) => Ok(Self::Never),
            Value::Name(name) => match name.as_str() {
                "auto" => Ok(Self::Auto),
                "left" => Ok(Self::Left),
                "right" => Ok(Self::Right),
                _ => Err(serde::de::Error::unknown_variant(&name, NAMES)),
            },
        }
    }
}
