//! CodeRabbit review activity, read by running the CodeRabbit CLI's own
//! `coderabbit usage` on the host, which uses the CLI's hosted login
//! (`coderabbit auth login`) and prints the organization, user, billing state,
//! review count, and billing-period reset. The report has no quota
//! denominator, so it shows the review count rather than a percentage. The
//! CLI owns its credentials; nothing else is read. Self-hosted logins are not
//! supported by the upstream command.

use crate::model::Count;
use crate::text::Text;
use crate::{
    Error, Result,
    {
        model::{Account, Balance, Provider, Report, Section, Unit},
        probe::Probe,
        service::{Meta, Service, Setting},
    },
};
use std::{collections::HashMap, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(15);
/// The CLI may print its report on standard error, which the probe drops, so
/// a shell merges it into standard output. The report holds no secrets.
const SCRIPT: &str = "\"$0\" usage 2>&1";

pub(crate) struct Coderabbit;

static META: Meta = Meta::new("coderabbit", "CodeRabbit")
    .dashboard("https://app.coderabbit.ai")
    .status_page("https://status.coderabbit.ai")
    .settings(&[Setting::new(
        "cli_path",
        &["CODERABBIT_CLI_PATH"],
        "usage-coderabbit-setting-cli-path",
    )]);

impl Service for Coderabbit {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let program = probe
            .text_setting("cli_path")
            .unwrap_or_else(|| "coderabbit".into());
        let output = probe
            .command("sh", &["-c", SCRIPT, program.as_str()], TIMEOUT)
            .ok()?;
        match parse(&output.stdout) {
            Some(report) => Some(Ok(report)),
            // A missing CLI or a signed-out one is no sign-in.
            None if !output.success || signed_out(&output.stdout) => None,
            None => Some(Err(Error::Command("read CodeRabbit usage"))),
        }
    }
}

/// None when the output has none of the usage fields.
pub(crate) fn parse(text: &str) -> Option<Report> {
    let text = strip_ansi(text);
    let mut fields: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        let (label, value) = (label.trim().to_lowercase(), value.trim());
        if !value.is_empty() {
            fields.entry(label).or_insert_with(|| value.to_owned());
        }
    }
    let reviews = fields
        .get("your reviews")
        .and_then(|reviews| reviews.parse::<u64>().ok());
    let billing = fields.get("usage billing").cloned();
    let resets = fields.get("period resets").cloned();
    if reviews.is_none() && billing.is_none() && resets.is_none() {
        return None;
    }
    let mut facts = Vec::new();
    // The values are the CLI's own, such as `inactive` for billing.
    let named = [
        (
            Text::message("usage-coderabbit-organization"),
            fields.get("organization"),
        ),
        (Text::message("usage-coderabbit-user"), fields.get("user")),
        (
            Text::message("usage-coderabbit-usage-billing"),
            billing.as_ref(),
        ),
        (
            Text::message("usage-coderabbit-period-resets"),
            resets.as_ref(),
        ),
    ];
    for (label, value) in named {
        if let Some(value) = value {
            facts.push((label, Text::service(value)));
        }
    }
    let account = Account {
        email: None,
        plan: fields.get("plan").map(Text::service),
    };
    Some(
        Report::new(Provider(&Coderabbit), account, Vec::new())
            .with_balances(reviews.map(|reviews| {
                Balance::new(
                    Text::message("usage-coderabbit-your-reviews"),
                    reviews as f64,
                    Unit::Count(Count::Reviews),
                )
            }))
            .with_sections((!facts.is_empty()).then(|| Section::Facts {
                title: Text::message("usage-coderabbit-billing-period"),
                facts,
            })),
    )
}

fn signed_out(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "not authenticated",
        "please log in",
        "auth login",
        "authentication required",
        "unauthorized",
        "no session found",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

/// Drops `ESC [ … letter` color sequences.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const USAGE: &str = "CodeRabbit Usage — current billing period\n\n\
        Organization  : Example Org\n\
        Usage billing : inactive\n\
        User          : example-user\n\
        Your reviews  : 25\n\
        Period resets : 2026-09-30\n";

    #[test]
    fn reads_the_usage_report() {
        let report = parse(USAGE).unwrap();
        assert!(report.windows.is_empty());
        assert_eq!(report.balances[0].amount, 25.);
        assert_eq!(report.balances[0].unit, Unit::Count(Count::Reviews));
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        assert_eq!(
            facts,
            &[
                (
                    Text::message("usage-coderabbit-organization"),
                    Text::service("Example Org")
                ),
                (
                    Text::message("usage-coderabbit-user"),
                    Text::service("example-user")
                ),
                (
                    Text::message("usage-coderabbit-usage-billing"),
                    Text::service("inactive")
                ),
                (
                    Text::message("usage-coderabbit-period-resets"),
                    Text::service("2026-09-30")
                ),
            ]
        );
    }

    #[test]
    fn keeps_zero_reviews_and_skips_empty_fields() {
        let report = parse("Organization :\nYour reviews : 0\n").unwrap();
        assert_eq!(report.balances[0].amount, 0.);
        assert!(report.sections.is_empty());
    }

    #[test]
    fn strips_colors_and_carriage_returns() {
        let report =
            parse("\u{1b}[32mYour reviews\u{1b}[0m: 12\r\nUsage billing: active\r\n").unwrap();
        assert_eq!(report.balances[0].amount, 12.);
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected facts");
        };
        assert_eq!(facts[0].1, "active");
    }

    #[test]
    fn needs_a_usage_field() {
        for text in [
            "Plan: Pro",
            "Organization: Example\nUser: example-user",
            "unexpected",
        ] {
            assert!(parse(text).is_none(), "{text}");
        }
        assert!(signed_out(
            "Error: not authenticated. Run coderabbit auth login."
        ));
    }
}
