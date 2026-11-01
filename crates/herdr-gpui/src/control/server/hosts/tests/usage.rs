use super::*;
use crate::usage::{
    Reading,
    model::{Account, Kind, Report, Window},
    registry,
};
use herdr_usage::Text;
use std::time::Duration;

fn reading(id: &str, windows: Vec<Window>) -> Reading {
    let provider = registry::find(id).unwrap();
    Reading {
        provider,
        report: Some(Report::new(provider, Account::default(), windows)),
        error: None,
        access: None,
    }
}

/// `herdr-gpui usage` answers each window's English title, which it prints
/// and `--json` sends as is: a named window's words with their arguments,
/// never the message ID the provider names.
#[test]
fn the_usage_command_prints_named_windows_in_english() {
    let now = SystemTime::now();
    let resets = Some(now + Duration::from_secs(3600));
    let copilot = reading(
        "copilot",
        vec![Window::new(
            Kind::Named(Text::message("usage-copilot-premium")),
            40.,
            resets,
            None,
        )],
    );
    let doubao = reading(
        "doubao",
        vec![
            Window::new(Kind::Weekly, 10., None, None),
            Window::new(
                Kind::Named(
                    Text::message("usage-doubao-window")
                        .arg("plan", "agent")
                        .arg("period", "weekly"),
                ),
                28.7,
                None,
                None,
            ),
        ],
    );
    let usage = [copilot, doubao]
        .iter()
        .map(|reading| usage_info(reading, now))
        .collect::<Vec<_>>();
    let labels: Vec<_> = usage
        .iter()
        .flat_map(|info| info.windows.iter().map(|window| window.label.as_str()))
        .collect();
    assert_eq!(labels, ["Premium", "Weekly", "Agent weekly"]);
    let json = serde_json::to_string(&Response::Host {
        reply: Reply::Usage {
            host: "Local".into(),
            usage,
            note: None,
        },
    })
    .unwrap();
    assert!(json.contains(r#""label":"Agent weekly""#), "{json}");
    assert!(!json.contains("usage-"), "{json}");
}
