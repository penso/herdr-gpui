use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

pub(super) struct Icons;

/// Icons embedded by this app, as opposed to the kit's `IconName` set.
const OWN: [&str; 21] = [
    "icons/agent-opencode.svg",
    "icons/agent-claude.svg",
    "icons/agent-codex.svg",
    "icons/agent-gemini.svg",
    "icons/agent-cursor.svg",
    "icons/agent-copilot.svg",
    "icons/agent-generic.svg",
    "icons/devices.svg",
    "icons/settings.svg",
    "icons/plus.svg",
    "icons/close.svg",
    "icons/user.svg",
    "icons/x.svg",
    "icons/pencil.svg",
    "icons/trash.svg",
    "icons/chevron-up.svg",
    "icons/chevron-down.svg",
    "icons/git-branch.svg",
    "icons/github.svg",
    "icons/theme.svg",
    "icons/keyboard.svg",
];

/// Canonical daemon identities, independent of editable display names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentIcon {
    OpenCode,
    Claude,
    Codex,
    Gemini,
    Cursor,
    Copilot,
    Generic,
}

impl AgentIcon {
    pub(crate) fn from_identity(identity: Option<&str>) -> Self {
        match identity {
            Some("opencode") => Self::OpenCode,
            Some("claude") => Self::Claude,
            Some("codex") => Self::Codex,
            Some("gemini") => Self::Gemini,
            Some("cursor") => Self::Cursor,
            Some("copilot") => Self::Copilot,
            _ => Self::Generic,
        }
    }

    pub(crate) fn path(self) -> &'static str {
        match self {
            Self::OpenCode => "icons/agent-opencode.svg",
            Self::Claude => "icons/agent-claude.svg",
            Self::Codex => "icons/agent-codex.svg",
            Self::Gemini => "icons/agent-gemini.svg",
            Self::Cursor => "icons/agent-cursor.svg",
            Self::Copilot => "icons/agent-copilot.svg",
            Self::Generic => "icons/agent-generic.svg",
        }
    }
}

impl AssetSource for Icons {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/agent-opencode.svg" => {
                include_bytes!("../../../assets/icons/agent-opencode.svg")
            }
            "icons/agent-claude.svg" => include_bytes!("../../../assets/icons/agent-claude.svg"),
            "icons/agent-codex.svg" => include_bytes!("../../../assets/icons/agent-codex.svg"),
            "icons/agent-gemini.svg" => include_bytes!("../../../assets/icons/agent-gemini.svg"),
            "icons/agent-cursor.svg" => include_bytes!("../../../assets/icons/agent-cursor.svg"),
            "icons/agent-copilot.svg" => include_bytes!("../../../assets/icons/agent-copilot.svg"),
            "icons/agent-generic.svg" => include_bytes!("../../../assets/icons/agent-generic.svg"),
            "icons/devices.svg" => include_bytes!("../../../assets/icons/devices.svg"),
            "icons/settings.svg" => include_bytes!("../../../assets/icons/settings.svg"),
            "icons/plus.svg" => include_bytes!("../../../assets/icons/plus.svg"),
            "icons/close.svg" => include_bytes!("../../../assets/icons/close.svg"),
            "icons/user.svg" => include_bytes!("../../../assets/icons/user.svg"),
            "icons/x.svg" => include_bytes!("../../../assets/icons/x.svg"),
            "icons/pencil.svg" => include_bytes!("../../../assets/icons/pencil.svg"),
            "icons/trash.svg" => include_bytes!("../../../assets/icons/trash.svg"),
            "icons/chevron-up.svg" => include_bytes!("../../../assets/icons/chevron-up.svg"),
            "icons/chevron-down.svg" => include_bytes!("../../../assets/icons/chevron-down.svg"),
            "icons/git-branch.svg" => include_bytes!("../../../assets/icons/git-branch.svg"),
            "icons/github.svg" => include_bytes!("../../../assets/icons/github.svg"),
            "icons/theme.svg" => include_bytes!("../../../assets/icons/theme.svg"),
            "icons/keyboard.svg" => include_bytes!("../../../assets/icons/keyboard.svg"),
            // Anything else is a gpui-kit icon (`IconName`), which the kit's
            // own components also load through this single app asset source.
            _ => return gpui_kit::assets::Assets.load(path),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        Ok(OWN
            .into_iter()
            .filter(|name| name.starts_with(path))
            .map(SharedString::from)
            .chain(gpui_kit::assets::Assets.list(path)?)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use gpui_kit::{DevicePixels, Image, ImageFormat, TestAppContext};

    #[gpui_kit::test]
    fn embedded_icons_render_nonempty_masks(cx: &mut TestAppContext) {
        let renderer = cx.update(|cx| cx.svg_renderer());
        for path in OWN {
            let bytes = Icons.load(path).unwrap().unwrap();
            // Decode through GPUI's SVG renderer; production uses svg() for tinting.
            let image = Image::from_bytes(ImageFormat::Svg, bytes.into_owned())
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
        // Unknown paths fall through to the kit, which reports them as errors.
        assert!(!matches!(Icons.load("unknown.svg"), Ok(Some(_))));
    }

    #[test]
    fn canonical_agent_identities_select_embedded_assets() {
        for (identity, expected) in [
            (Some("opencode"), AgentIcon::OpenCode),
            (Some("claude"), AgentIcon::Claude),
            (Some("codex"), AgentIcon::Codex),
            (Some("gemini"), AgentIcon::Gemini),
            (Some("cursor"), AgentIcon::Cursor),
            (Some("copilot"), AgentIcon::Copilot),
            (Some("future-agent"), AgentIcon::Generic),
            (Some("Claude Code"), AgentIcon::Generic),
            (Some(""), AgentIcon::Generic),
            (None, AgentIcon::Generic),
        ] {
            let icon = AgentIcon::from_identity(identity);
            assert_eq!(icon, expected);
            assert!(Icons.load(icon.path()).unwrap().is_some());
            assert!(
                Icons
                    .list("icons/agent-")
                    .unwrap()
                    .iter()
                    .any(|path| path == icon.path())
            );
        }
    }
}
