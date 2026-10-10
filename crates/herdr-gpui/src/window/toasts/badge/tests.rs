use super::*;
use crate::{icons::AgentIcon, notifications::tests::notification};
use std::time::Instant;

fn notice(title: &str, agent: Option<&str>, kind: SemanticNotificationKind) -> Notice {
    let mut wire = notification(title);
    wire.agent = agent.map(Into::into);
    wire.kind = kind;
    Notice::new(wire, Instant::now())
}

#[test]
fn a_leading_emoji_becomes_the_mark_and_leaves_the_title() {
    for (title, emoji, rest) in [
        ("🚀 Deployed", "🚀", "Deployed"),
        ("⚠️  Disk almost full", "⚠️", "Disk almost full"),
        ("👩‍💻 Review ready", "👩‍💻", "Review ready"),
        ("🇫🇷 Bonjour", "🇫🇷", "Bonjour"),
        ("👍🏽 Approved", "👍🏽", "Approved"),
    ] {
        let notice = notice(title, Some("claude"), SemanticNotificationKind::Finished);
        assert_eq!(
            badge(&notice),
            Badge {
                mark: Mark::Emoji(emoji),
                title: rest,
            },
            "{title}"
        );
    }
}

#[test]
fn titles_without_a_separate_leading_emoji_keep_their_text() {
    for title in [
        "🚀",
        "🚀   ",
        "🚀Deployed",
        "Deployed 🚀",
        "A note",
        "→ arrow",
        "界 wide text",
    ] {
        let notice = notice(title, None, SemanticNotificationKind::Custom);
        assert_eq!(badge(&notice).mark, Mark::Icon("icons/note.svg"), "{title}");
        assert_eq!(badge(&notice).title, notice.title, "{title}");
    }
}

#[test]
fn a_known_agent_shows_its_mark_before_the_kind_icon() {
    let claude = notice(
        "Waiting",
        Some("claude"),
        SemanticNotificationKind::Finished,
    );
    assert_eq!(claude.agent, Some(AgentIcon::Claude));
    assert_eq!(badge(&claude).mark, Mark::Icon(AgentIcon::Claude.path()));
    // A display-cased identity still resolves; an unknown one falls back.
    let codex = notice("Waiting", Some("Codex"), SemanticNotificationKind::Finished);
    assert_eq!(codex.agent, Some(AgentIcon::Codex));
    for agent in [Some("untrusted"), Some(&*"claude".repeat(20)), None] {
        let notice = notice("Waiting", agent, SemanticNotificationKind::NeedsAttention);
        assert_eq!(notice.agent, None);
        assert_eq!(badge(&notice).mark, Mark::Icon("icons/bell.svg"));
    }
}

#[test]
fn each_kind_has_its_own_fallback_icon() {
    for (kind, icon) in [
        (SemanticNotificationKind::NeedsAttention, "icons/bell.svg"),
        (SemanticNotificationKind::Finished, "icons/check.svg"),
        (
            SemanticNotificationKind::UpdateInstalled,
            "icons/refresh.svg",
        ),
        (SemanticNotificationKind::Custom, "icons/note.svg"),
    ] {
        assert_eq!(badge(&notice("Done", None, kind)).mark, Mark::Icon(icon));
    }
}
