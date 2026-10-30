use super::{RowContext, RowKind, TokenLook, left_behind, token_appearance};
use crate::config::{FontConfig, LayoutMode, Theme};
use crate::sidebar::tokens::{TextRole, TokenKind};
use crate::sidebar::{agents::Indicators, layout};
use herdr_client::protocol::AgentStatus;

#[core::prelude::v1::test]
fn teleported_rows_fade_only_their_workspace_tokens() {
    let font = FontConfig {
        family: "Menlo".into(),
        size: 12.,
        fallbacks: None,
        line_height_multiple: None,
        weight: gpui::FontWeight::NORMAL,
    };
    let theme = Theme::default();
    let cx = RowContext {
        indicators: Indicators::new(
            None,
            crate::config::StatusIcons::default(),
            false,
            false,
            &theme,
        ),
        font: &font,
        theme: &theme,
        look: layout::for_mode(LayoutMode::default()),
        width: 232.,
        nest: 0.,
        mark: Default::default(),
        host: None,
    };
    let here = TokenLook {
        kind: RowKind::Workspace,
        status: AgentStatus::Idle,
        focused: false,
        teleported: false,
    };
    let away = TokenLook {
        teleported: true,
        ..here
    };
    let workspace = TokenKind::Text("repo".into(), TextRole::Workspace);
    let (name, weight) = token_appearance(&workspace, here, &cx);
    assert_eq!(
        token_appearance(&workspace, away, &cx),
        (left_behind(name, &theme), weight)
    );
    assert_ne!(left_behind(name, &theme), name);
    for other in [
        TokenKind::Text("idle".into(), TextRole::Status),
        TokenKind::Text("main".into(), TextRole::Secondary),
        TokenKind::StateIcon,
    ] {
        assert_eq!(
            token_appearance(&other, away, &cx),
            token_appearance(&other, here, &cx)
        );
    }
}
