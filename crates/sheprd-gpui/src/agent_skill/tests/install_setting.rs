use super::*;

fn installed() -> AgentSkill {
    AgentSkill {
        choice: Some(Choice::Installed),
        ask: true,
        install: None,
        path: None,
        refresh: false,
    }
}

#[gpui::test]
fn nothing_is_offered_or_sent_to_hosts_before_the_setting_loads(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(installed());
        assert_eq!(AgentSkill::remote_choice(cx), None);
        assert!(!AgentSkill::take_ask(cx));
        // The setting arrives: asking is allowed again, once.
        AgentSkill::allow(cx, SkillInstall::Ask);
        assert_eq!(AgentSkill::remote_choice(cx), Some(Choice::Installed));
        assert!(AgentSkill::take_ask(cx));
        assert!(!AgentSkill::take_ask(cx));
    });
}

#[gpui::test]
fn local_keeps_ssh_hosts_out_and_off_also_stops_the_offer(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(installed());
        AgentSkill::allow(cx, SkillInstall::Local);
        assert_eq!(AgentSkill::remote_choice(cx), None);
        assert_eq!(AgentSkill::choice(cx), Some(Choice::Installed));

        AgentSkill::allow(cx, SkillInstall::Off);
        assert_eq!(AgentSkill::remote_choice(cx), None);
        assert!(!AgentSkill::take_ask(cx), "off never asks");
        // Turning it back on later still asks, as the question was not taken.
        AgentSkill::allow(cx, SkillInstall::Local);
        assert!(AgentSkill::take_ask(cx));
    });
}

#[gpui::test]
fn off_leaves_the_installed_skill_unrefreshed(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(AgentSkill {
            refresh: true,
            ..installed()
        });
        AgentSkill::allow(cx, SkillInstall::Off);
        AgentSkill::allow(cx, SkillInstall::Off);
        // Allowing it would rewrite this machine's skill, so stop here.
        assert!(cx.global::<AgentSkill>().refresh);
    });
}

#[gpui::test]
fn removing_the_skill_cancels_the_refresh_off_held_back(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(AgentSkill {
            refresh: true,
            ..installed()
        });
        AgentSkill::allow(cx, SkillInstall::Off);
        AgentSkill::choose(Choice::Declined, cx);
        assert!(!cx.global::<AgentSkill>().refresh);
        // So allowing it again rewrites nothing.
        AgentSkill::allow(cx, SkillInstall::Ask);
        assert_eq!(AgentSkill::choice(cx), Some(Choice::Declined));
    });
}
