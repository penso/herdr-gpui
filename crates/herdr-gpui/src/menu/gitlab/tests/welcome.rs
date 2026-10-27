//! The welcome page's Connect: it offers the instance this window reads,
//! not merely the first one listed across every window.
use super::*;
use crate::menu::gitlab::GitLabMenu;

fn instance(host: &str, can_connect: bool) -> HostStatus {
    HostStatus {
        host: host.into(),
        can_connect,
        ..status(None, None)
    }
}

#[test]
fn the_welcome_page_connects_the_instance_this_window_reads() {
    let mut menu = GitLabMenu {
        hosts: vec![
            instance("a.example.com", true),
            instance("b.example.com", false),
            instance("c.example.com", true),
        ],
        ..GitLabMenu::default()
    };
    // No instance of this window's: the first that can connect.
    assert_eq!(menu.welcome_lead(), 0);
    menu.preferred = vec!["c.example.com".into()];
    assert_eq!(menu.welcome_lead(), 2);
    // This window's instance cannot connect: the first that can.
    menu.preferred = vec!["b.example.com".into()];
    assert_eq!(menu.welcome_lead(), 0);
    menu.hosts = vec![instance("a.example.com", false)];
    assert_eq!(menu.welcome_lead(), 0);
}
