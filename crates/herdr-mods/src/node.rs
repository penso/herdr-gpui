//! The element tree a mod's `view` returns, checked into types the app can
//! draw without running Lua. Styles are theme tokens, never colors or
//! pixels, so a mod follows the user's theme and the app's spacing.

use crate::{Data, names::named};
use herdr_commands::Command;

mod read;

pub(crate) use read::{command_named, read};

named! {
    pub enum Element {
        Badge = "badge",
        Button = "button",
        Column = "column",
        Divider = "divider",
        Icon = "icon",
        Link = "link",
        List = "list",
        Meter = "meter",
        Progress = "progress",
        Row = "row",
        Spacer = "spacer",
        Text = "text",
        Toggle = "toggle",
        Tooltip = "tooltip",
    }
}

impl Element {
    /// The keys an element takes besides `type`, so a typo is reported as
    /// the unknown key it is rather than as the key it meant.
    pub fn keys(self) -> &'static [&'static str] {
        match self {
            Self::Badge => &["text", "tone"],
            Self::Button => &[
                "label", "name", "file", "tone", "command", "msg", "disabled",
            ],
            Self::Column | Self::Row => &["gap", "align"],
            Self::Divider | Self::List | Self::Spacer => &[],
            Self::Icon => &["name", "file", "tone"],
            Self::Link => &["label", "url"],
            Self::Meter => &["value", "label", "tone"],
            Self::Progress => &["value", "tone"],
            Self::Text => &["text", "tone", "size", "weight", "truncate"],
            Self::Toggle => &["label", "on", "msg"],
            Self::Tooltip => &["text"],
        }
    }
}

named! {
    /// A theme role for text, icons, and fills.
    pub enum Tone {
        Normal = "normal",
        Muted = "muted",
        Accent = "accent",
        Success = "success",
        Warning = "warning",
        Danger = "danger",
        /// Needs the user: the theme's color for blocked agents.
        Attention = "attention",
    }
}

named! {
    pub enum Size {
        Small = "small",
        Normal = "normal",
        Large = "large",
    }
}

named! {
    pub enum Weight {
        Normal = "normal",
        Bold = "bold",
    }
}

named! {
    /// Space between a container's children, from the app's spacing scale.
    pub enum Gap {
        None = "none",
        Small = "small",
        Normal = "normal",
        Large = "large",
    }
}

named! {
    /// Where a container's children sit across its axis. They stretch to
    /// fill it unless told otherwise, so bars and lists span a section.
    pub enum Align {
        Stretch = "stretch",
        Start = "start",
        Center = "center",
        End = "end",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Badge(Badge),
    Button(Button),
    Column(Container),
    Divider,
    Icon(Icon),
    Link(Link),
    List(List),
    Meter(Meter),
    Progress(Progress),
    Row(Container),
    Spacer,
    Text(Text),
    Toggle(Toggle),
    Tooltip(Tooltip),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Container {
    pub gap: Gap,
    pub align: Align,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub text: String,
    pub tone: Tone,
    pub size: Size,
    pub weight: Weight,
    /// Cut to one line with an ellipsis rather than wrap.
    pub truncate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconSource {
    /// One of the app's own icons, by name.
    App(String),
    /// An SVG file in the mod's folder.
    File(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon {
    pub source: IconSource,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Badge {
    pub text: String,
    pub tone: Tone,
}

/// What clicking a control does.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A command from the app's catalog, as the palette runs it.
    Command(Command),
    /// A message for the mod's `update`.
    Message(Data),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub label: Option<String>,
    pub icon: Option<IconSource>,
    pub tone: Tone,
    pub action: Action,
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct List {
    pub items: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    /// From 0 to 1; `None` while the amount is unknown.
    pub value: Option<f64>,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Meter {
    /// From 0 to 1.
    pub value: f64,
    pub label: Option<String>,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub label: String,
    /// An `https://` URL; the app opens it in a browser tab.
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toggle {
    pub label: String,
    pub on: bool,
    /// Sent to `update` when the toggle is clicked.
    pub msg: Data,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tooltip {
    pub text: String,
    pub child: Box<Node>,
}

impl Node {
    /// This node and every node under it.
    pub fn count(&self) -> usize {
        1 + match self {
            Self::Row(container) | Self::Column(container) => {
                container.children.iter().map(Self::count).sum()
            }
            Self::List(list) => list.items.iter().map(Self::count).sum(),
            Self::Tooltip(tooltip) => tooltip.child.count(),
            _ => 0,
        }
    }
}
