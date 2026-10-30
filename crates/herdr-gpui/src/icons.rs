use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub(super) struct Icons;

/// Declares each agent mark once: its variant, Herdr's canonical
/// `agent_label` identity, and the embedded asset named after that identity.
macro_rules! agent_icons {
    ($($variant:ident => $label:literal, $extension:literal,)+) => {
        /// Canonical daemon identities, independent of editable display names.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum AgentIcon {
            $($variant,)+
            Generic,
        }

        impl AgentIcon {
            const ALL: &[Self] = &[$(Self::$variant,)+ Self::Generic];

            pub(crate) fn from_identity(identity: Option<&str>) -> Self {
                match identity {
                    $(Some($label) => Self::$variant,)+
                    _ => Self::Generic,
                }
            }

            pub(crate) fn path(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!("icons/agent-", $label, ".", $extension),)+
                    Self::Generic => "icons/agent-generic.svg",
                }
            }

            fn bytes(self) -> &'static [u8] {
                match self {
                    $(Self::$variant => include_bytes!(concat!(
                        "../../../assets/icons/agent-", $label, ".", $extension
                    )),)+
                    Self::Generic => include_bytes!("../../../assets/icons/agent-generic.svg"),
                }
            }

            pub(crate) fn element(self, size: f32, color: u32) -> gpui::AnyElement {
                icon(self.path(), size, color)
            }
        }
    };
}

// Keyed on Herdr's `src/detect/mod.rs` `agent_label`, in its declaration order.
agent_icons! {
    Pi => "pi", "svg",
    Omo => "omo", "png",
    Claude => "claude", "svg",
    Codex => "codex", "svg",
    Gemini => "gemini", "svg",
    Cursor => "cursor", "svg",
    Devin => "devin", "svg",
    Antigravity => "agy", "svg",
    Cline => "cline", "svg",
    Omp => "omp", "svg",
    Mastracode => "mastracode", "svg",
    OpenCode => "opencode", "svg",
    Copilot => "copilot", "svg",
    Kimi => "kimi", "svg",
    Kiro => "kiro", "svg",
    Droid => "droid", "svg",
    Amp => "amp", "svg",
    Grok => "grok", "svg",
    Hermes => "hermes", "svg",
    Kilo => "kilo", "svg",
    Qodercli => "qodercli", "svg",
    Qwen => "qwen", "svg",
    Letta => "letta", "svg",
    Maki => "maki", "svg",
    Muse => "muse", "svg",
}

/// Raster marks retain their source colors; SVG glyphs use the caller's ink.
pub(crate) fn icon(path: impl Into<SharedString>, size: f32, color: u32) -> gpui::AnyElement {
    use gpui::{ObjectFit, img, prelude::*, px, rgb, svg};
    let path = path.into();
    if path.ends_with(".png") {
        img(path)
            .size(px(size))
            .flex_none()
            .object_fit(ObjectFit::Contain)
            .into_any_element()
    } else {
        svg()
            .path(path)
            .size(px(size))
            .flex_none()
            .text_color(rgb(color))
            .into_any_element()
    }
}

/// Embeds each `status_icons` set, one asset per status, in the order
/// `status_icon` indexes: sets as `StatusIcons` declares them, then statuses.
macro_rules! status_icons {
    ($($set:literal),+) => {
        const STATUS_ICONS: [(&str, &[u8]); 15] = [$(
            status_icons!(@one $set, "idle"),
            status_icons!(@one $set, "working"),
            status_icons!(@one $set, "done"),
            status_icons!(@one $set, "blocked"),
            status_icons!(@one $set, "unknown"),
        )+];
    };
    (@one $set:literal, $status:literal) => {
        (
            concat!("icons/status-", $set, "-", $status, ".svg"),
            include_bytes!(concat!("../../../assets/icons/status-", $set, "-", $status, ".svg")),
        )
    };
}

// Solid, then tiles' inner mark, then the outline set.
status_icons!("solid", "mark", "outline");

/// The asset that draws `status` in a `status_icons` set, or `None` for the
/// sets whose marks are drawn rather than loaded.
pub(crate) fn status_icon(
    icons: crate::config::StatusIcons,
    status: herdr_client::protocol::AgentStatus,
) -> Option<&'static str> {
    use crate::config::StatusIcons;
    use herdr_client::protocol::AgentStatus;
    let set = match icons {
        StatusIcons::Solid => 0,
        StatusIcons::Tiles => 1,
        StatusIcons::Outline => 2,
        _ => return None,
    };
    let status = match status {
        AgentStatus::Idle => 0,
        AgentStatus::Working => 1,
        AgentStatus::Done => 2,
        AgentStatus::Blocked => 3,
        AgentStatus::Unknown => 4,
    };
    Some(STATUS_ICONS[set * 5 + status].0)
}

/// Shared working-tree marker, distinct from the daemon's activity dots.
pub(super) fn uncommitted(theme: &crate::config::Theme, size: f32) -> gpui::Div {
    use gpui::{div, prelude::*, px, rgb, rgba, svg};
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(crate::config::corners::SMALL))
        .bg(rgba((theme.palette[3] << 8) | 0x30))
        .border_1()
        .border_color(rgba((theme.palette[3] << 8) | 0x90))
        .child(
            svg()
                .path("icons/pencil.svg")
                .size(px(size - 4.))
                .text_color(rgb(theme.ink(theme.palette[3]))),
        )
}

/// A checkout the user keeps a note on, beside the working-tree marker and
/// in a hue of its own so the two never read as one.
pub(super) fn note(theme: &crate::config::Theme, size: f32) -> gpui::Div {
    use gpui::{div, prelude::*, px, rgb, rgba, svg};
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(crate::config::corners::SMALL))
        .bg(rgba((theme.palette[4] << 8) | 0x30))
        .border_1()
        .border_color(rgba((theme.palette[4] << 8) | 0x90))
        .child(
            svg()
                .path("icons/note.svg")
                .size(px(size - 4.))
                .text_color(rgb(theme.ink(theme.palette[4]))),
        )
}

/// A theme the user starred, and the outline offering to star one.
pub(crate) const STARRED: &str = "icons/star-full.svg";
pub(crate) const UNSTARRED: &str = "icons/star.svg";

/// The mark of a workspace or tab whose notifications the user muted.
pub(crate) const MUTED: &str = "icons/bell-off.svg";

/// `mark` named "Notifications muted" for screen readers, which hear nothing
/// of an icon: for a muted mark in a row that carries no accessible name of
/// its own.
pub(crate) fn spoken_muted(
    id: impl Into<gpui::ElementId>,
    mark: impl gpui::IntoElement,
) -> gpui::Stateful<gpui::Div> {
    use gpui::{Role, div, prelude::*};
    div()
        .id(id)
        .role(Role::Image)
        .aria_label("Notifications muted")
        .flex_none()
        .child(mark)
}

/// A workspace whose notifications the user muted: the bell struck through,
/// in the theme's quiet greys rather than a hue, so it reads as silence
/// beside the other marks. The glyph fills more of its box than theirs do,
/// since a slash through a bell blurs sooner than a pencil.
pub(crate) fn muted(theme: &crate::config::Theme, size: f32) -> gpui::Div {
    use gpui::{div, prelude::*, px, rgb, rgba, svg};
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(crate::config::corners::SMALL))
        .bg(rgba((theme.muted << 8) | 0x24))
        .border_1()
        .border_color(rgba((theme.muted << 8) | 0x70))
        .child(
            svg()
                .path(MUTED)
                .size(px(size - 2.))
                .text_color(rgb(theme.subtext())),
        )
}

/// A checkout whose work was teleported to another host.
pub(super) fn teleported(theme: &crate::config::Theme, size: f32) -> gpui::Div {
    use gpui::{Rgba, div, prelude::*, px, svg};
    let color = crate::menu::teleported(theme);
    let faded = |a: f32| Rgba { a, ..color };
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(crate::config::corners::SMALL))
        .bg(faded(0.22))
        .border_1()
        .border_color(faded(0.7))
        .child(
            svg()
                .path("icons/teleport.svg")
                .size(px(size - 4.))
                .text_color(color),
        )
}

/// Declares each embedded UI icon once, so loading and listing cannot drift.
macro_rules! embedded_icons {
    ($($path:literal,)+) => {
        const EMBEDDED: &[(&str, &[u8])] = &[$((
            concat!("icons/", $path),
            include_bytes!(concat!("../../../assets/icons/", $path)),
        ),)+];
    };
}

// Embedded UI icons under assets/icons, one per line; agent marks and usage
// providers declare their own. Keep the list sorted so parallel additions land
// on different lines.
embedded_icons! {
    // keep-sorted start
    "arrow-left.svg",
    "arrow-right.svg",
    "arrow-up.svg",
    "automations.svg",
    "beads.svg",
    "bell-off.svg",
    "bell.svg",
    "chart.svg",
    "check.svg",
    "chevron-down.svg",
    "chevron-left.svg",
    "chevron-right.svg",
    "chevron-up.svg",
    "close.svg",
    "cloud/azure.svg",
    "cloud/coder.svg",
    "cloud/container.svg",
    "cloud/daytona.svg",
    "cloud/opensandbox.svg",
    "cloud/railway.svg",
    "cloud/sprites.svg",
    "code.svg",
    "coffee-full.svg",
    "coffee.svg",
    "computer.svg",
    "copy.svg",
    "devices.svg",
    "diff-split.svg",
    "diff-unified.svg",
    "error.svg",
    "external.svg",
    "fan-out.svg",
    "file.svg",
    "git-branch.svg",
    "github.svg",
    "gitlab.svg",
    "globe.svg",
    "issue.svg",
    "keyboard.svg",
    "lock.svg",
    "more.svg",
    "network/tailscale.svg",
    "note.svg",
    "panel-left.svg",
    "panel-right.svg",
    "pass.svg",
    "pencil.svg",
    "phone.svg",
    "play.svg",
    "plug.svg",
    "plus.svg",
    "pull-request.svg",
    "pulse.svg",
    "refresh.svg",
    "search.svg",
    "sessions.svg",
    "settings.svg",
    "shield.svg",
    "split.svg",
    "star-full.svg",
    "star.svg",
    "status-bar.svg",
    "status-orbit.svg",
    "status-orca-blocked.svg",
    "status-orca-working.svg",
    "tabs.svg",
    "teleport-back.svg",
    "teleport.svg",
    "terminal.svg",
    "theme.svg",
    "trash.svg",
    "user.svg",
    "vscode.svg",
    "whitespace.svg",
    "window-maximize.svg",
    "window-minimize.svg",
    "window-restore.svg",
    "x.svg",
    "zoom.svg",
    // keep-sorted end
}

impl AssetSource for Icons {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if let Some(icon) = AgentIcon::ALL.iter().find(|icon| icon.path() == path) {
            return Ok(Some(Cow::Borrowed(icon.bytes())));
        }
        let bytes = EMBEDDED
            .iter()
            .chain(&STATUS_ICONS)
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| *bytes)
            .or_else(|| crate::usage::icon(path));
        Ok(bytes.map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(EMBEDDED
            .iter()
            .map(|(name, _)| *name)
            .chain(AgentIcon::ALL.iter().map(|icon| icon.path()))
            .chain(STATUS_ICONS.iter().map(|(name, _)| *name))
            .chain(crate::usage::icon_paths())
            .filter(|name| name.starts_with(path))
            .map(Into::into)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use gpui::{DevicePixels, Image, ImageFormat, TestAppContext};

    #[gpui::test]
    fn embedded_icons_render_nonempty_masks(cx: &mut TestAppContext) {
        let renderer = cx.update(|cx| cx.svg_renderer());
        for path in Icons.list("icons/").unwrap() {
            let bytes = Icons.load(&path).unwrap().unwrap();
            let format = if path.ends_with(".png") {
                ImageFormat::Png
            } else {
                ImageFormat::Svg
            };
            let image = Image::from_bytes(format, bytes.into_owned())
                .to_image_data(renderer.clone())
                .unwrap();
            // Square at its own scale: the tab and menu glyphs are drawn on a
            // 24px grid, GitHub's mark on its own 16px one.
            let rendered = image.size(0);
            assert_eq!(rendered.width, rendered.height, "{path}");
            assert!(rendered.width >= DevicePixels(16), "{path}");
            let pixels = image.as_bytes(0).unwrap();
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
        }
        assert!(Icons.load("unknown.svg").unwrap().is_none());
        // Every asset is listed once, whichever table declares it.
        let listed = Icons.list("icons/").unwrap();
        let unique: std::collections::HashSet<_> = listed.iter().collect();
        assert_eq!(unique.len(), listed.len());
        for (name, _) in STATUS_ICONS {
            assert!(listed.iter().any(|path| path.as_ref() == name), "{name}");
        }
    }

    /// Herdr's `agent_label` values, copied from `src/detect/mod.rs` at
    /// herdrdev/herdr 65e35a38, plus the canonical native Omo identity.
    const HERDR_AGENT_LABELS: &[&str] = &[
        "pi",
        "omo",
        "claude",
        "codex",
        "gemini",
        "cursor",
        "devin",
        "agy",
        "cline",
        "omp",
        "mastracode",
        "opencode",
        "copilot",
        "kimi",
        "kiro",
        "droid",
        "amp",
        "grok",
        "hermes",
        "kilo",
        "qodercli",
        "qwen",
        "letta",
        "maki",
        "muse",
    ];

    #[test]
    fn every_herdr_agent_label_has_its_own_embedded_mark() {
        let listed = Icons.list("icons/agent-").unwrap();
        let mut seen = Vec::new();
        for label in HERDR_AGENT_LABELS {
            let icon = AgentIcon::from_identity(Some(label));
            assert_ne!(icon, AgentIcon::Generic, "{label}");
            let extension = if icon == AgentIcon::Omo { "png" } else { "svg" };
            assert_eq!(icon.path(), format!("icons/agent-{label}.{extension}"));
            assert!(Icons.load(icon.path()).unwrap().is_some(), "{label}");
            assert!(listed.iter().any(|path| path == icon.path()), "{label}");
            assert!(!seen.contains(&icon), "{label}");
            seen.push(icon);
        }
        // No marks for labels Herdr does not emit, beyond the fallback.
        assert_eq!(AgentIcon::ALL.len(), HERDR_AGENT_LABELS.len() + 1);
        assert_eq!(listed.len(), AgentIcon::ALL.len());
    }

    #[test]
    fn unknown_or_display_identities_use_the_generic_mark() {
        for identity in [
            Some("future-agent"),
            Some("Claude Code"),
            Some("kiro-cli"),
            Some("antigravity"),
            Some(""),
            None,
        ] {
            assert_eq!(AgentIcon::from_identity(identity), AgentIcon::Generic);
        }
        assert!(Icons.load(AgentIcon::Generic.path()).unwrap().is_some());
    }
}

#[cfg(test)]
#[path = "icons/tests/omo.rs"]
mod omo_tests;
