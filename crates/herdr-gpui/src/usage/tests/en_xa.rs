//! The `en-XA` leak test for plan usage: the status bar's segments, the
//! panel a segment opens (with a report and claude-swap's other accounts,
//! OpenAI's daily spend, waiting on the Keychain, and asking to be set up),
//! and the activity bar's Usage section.
use super::*;
use crate::{
    config::status_bar::Detail,
    i18n::leak_tests::{assert_translated, uncut, under_en_xa},
    sidebar::{
        ActivityPanel,
        layout_tests::{fixture_window, full_draw},
    },
    usage::{Access, Host, Usage},
};
use gpui::{VisualTestContext, point, px, size};
use herdr_usage::{
    details::{ClaudeAccount, ClaudeAccountStatus, OpenaiDaily},
    model::{Balance, Unit},
};
use std::time::{Duration, SystemTime};

/// Draws the whole window again: a view's own notify does not redraw a
/// headless root.
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx.run_until_parked();
}

/// What a step newly drew, without the cuts of lines drawn whole in this
/// step or an earlier one: GPUI shapes a line once and reuses it, so the
/// whole of a cut it measures again may be in `seen` alone.
fn fresh(seen: &mut Vec<String>, drawn: Vec<String>) -> Vec<String> {
    let kept = uncut(seen.iter().chain(&drawn).cloned().collect());
    seen.extend(drawn.iter().cloned());
    drawn
        .into_iter()
        .filter(|line| kept.contains(line))
        .collect()
}

const HOUR: Duration = Duration::from_secs(3600);

/// A report with every kind of line the panel draws: a session window on
/// pace, a weekly one ahead of it, a window the service names, and a
/// balance with a total.
fn full(provider: Provider) -> Report {
    let now = SystemTime::now();
    let mut report = Report::new(
        provider,
        Account {
            email: Some("someone@example.com".into()),
            plan: Some("Pro".into()),
        },
        vec![
            Window::new(Kind::Session, 40., Some(now + 3 * HOUR), Some(5 * HOUR)),
            Window::new(
                Kind::Weekly,
                90.,
                Some(now + 24 * 3 * HOUR),
                Some(24 * 7 * HOUR),
            ),
            Window::new(Kind::Named("Opus".into()), 10., None, None),
        ],
    );
    report
        .balances
        .push(Balance::new("Credits", 12.3, Unit::Currency("USD".into())).out_of(50.));
    report
}

/// A provider that reads a setting, so its panel shows how to set it up
/// while it has no report.
fn configurable() -> Provider {
    registry::all()
        .find(|provider| !provider.service().meta().settings.is_empty())
        .unwrap()
}

/// claude-swap's other accounts: one in rotation, one held out with an
/// expired token, and one held out and fine.
fn others() -> Vec<ClaudeAccount> {
    let account = |email: &str, disabled, status| ClaudeAccount {
        email: email.into(),
        alias: None,
        disabled,
        status,
        stale: false,
        windows: vec![Window::new(Kind::Weekly, 30., None, None)],
    };
    vec![
        account("one@example.com", false, ClaudeAccountStatus::Ok),
        account("two@example.com", true, ClaudeAccountStatus::TokenExpired),
        account("three@example.com", true, ClaudeAccountStatus::Ok),
    ]
}

fn readings() -> Vec<Reading> {
    let claude = provider("claude");
    let openai = provider("openai");
    let daily = OpenaiDaily {
        days: vec![("Oct 28".into(), 1.5), ("Oct 29".into(), 3.25)],
    };
    vec![
        Reading {
            provider: claude,
            report: Some(full(claude).with_detail(others())),
            error: None,
            access: None,
        },
        Reading {
            provider: openai,
            report: Some(full(openai).with_detail(daily)),
            error: None,
            access: None,
        },
        Reading {
            provider: provider("codex"),
            report: None,
            error: None,
            access: Some(Access::Needed),
        },
        Reading {
            provider: provider("copilot"),
            report: None,
            error: None,
            access: Some(Access::Denied),
        },
        Reading {
            provider: configurable(),
            report: None,
            error: None,
            access: None,
        },
    ]
}

/// What the services report and the user named: services, accounts, the
/// host, the named window, the balance's label, and each setting's key and
/// help, which `herdr-usage` writes.
fn data(readings: &[Reading], host: &str) -> Vec<String> {
    let mut data: Vec<String> = vec![
        host.into(),
        "someone@example.com".into(),
        "Pro".into(),
        "Opus".into(),
        "Credits".into(),
        "one@example.com".into(),
        "two@example.com".into(),
        "three@example.com".into(),
        "Oct 28".into(),
        "Oct 29".into(),
    ];
    for reading in readings {
        let meta = reading.provider.service().meta();
        data.push(meta.name.into());
        data.push(reading.provider.name().into());
        for setting in meta.settings {
            data.push(setting.help.into());
            data.push(format!("[usage.providers.{}] {}", meta.id, setting.name));
        }
    }
    data
}

#[test]
fn usage_shows_only_messages_and_what_services_report() {
    under_en_xa("usage en-XA leak", |cx, recorder| {
        let readings = readings();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = fixture_window(window, cx);
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.config.sidebar_style.activity_bar = true;
            view.config.status_bar.usage = Detail::Detailed;
            // Shown once the rest of the window is drawn.
            view.config.usage.show = false;
            view.usage = Usage::showing(Host::Local, readings.clone());
            // Tabs without numbers show only when asked for.
            view.config.usage.show_providers = readings
                .iter()
                .map(|reading| reading.provider.id().to_owned())
                .collect();
            view
        });
        cx.simulate_resize(size(px(2400.), px(900.)));
        let host = cx.update(|_, cx| view.read(cx).endpoints[0].label.clone());
        let data = data(&readings, &host);
        // The window around usage is other modules' text.
        draw(cx);
        let mut seen = recorder.take();

        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.config.usage.show = true;
                cx.notify();
            })
        });
        draw(cx);
        assert_translated(
            "status bar usage",
            &fresh(&mut seen, recorder.take()),
            &data,
        );

        for reading in &readings {
            let provider = reading.provider;
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.open_usage(provider, point(px(400.), px(800.)), window, cx)
                })
            });
            draw(cx);
            assert_translated(
                &format!("usage panel for {}", provider.id()),
                &fresh(&mut seen, recorder.take()),
                &data,
            );
        }

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.dismiss_menu(window, cx);
                view.show_activity_panel(ActivityPanel::Usage, window, cx);
            })
        });
        draw(cx);
        assert_translated("Usage section", &fresh(&mut seen, recorder.take()), &data);
    });
}
