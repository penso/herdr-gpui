//! The words batch A's providers name, written by the app: each message
//! reads in English as the provider used to write it.
use super::super::text;
use herdr_usage::Text;

/// Drops Fluent's isolation marks around placeables.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn english(words: Text) -> String {
    plain(text::words(&words))
}

#[test]
fn abacus_reads_as_it_did() {
    assert_eq!(english(Text::message("usage-abacus-credits")), "Credits");
    assert_eq!(
        english(Text::message("usage-abacus-credits-used")),
        "Credits used"
    );
}

#[test]
fn aiand_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-aiand-spend")),
        "Spend, last 30 days"
    );
    assert_eq!(
        english(Text::message("usage-aiand-spend-partial")),
        "Spend, last 30 days (partial)"
    );
}

#[test]
fn aixy_reads_as_it_did() {
    let budget = |scope: &str, interval: &str, sharing: &str, enforcement: &str| {
        english(
            Text::message("usage-aixy-budget")
                .arg("scope", scope)
                .arg("interval", interval)
                .arg("sharing", sharing)
                .arg("enforcement", enforcement),
        )
    };
    assert_eq!(
        budget("project", "monthly", "shared", "hard"),
        "Project · Monthly · Shared · Hard"
    );
    assert_eq!(
        budget("organization", "daily", "personal", "monitor"),
        "Organization · Daily · Personal · Monitor"
    );
    assert_eq!(
        budget("team", "weekly", "shared", "hard"),
        "Team · Weekly · Shared · Hard"
    );
    assert_eq!(
        budget("user", "lifetime", "personal", "monitor"),
        "User · Lifetime · Personal · Monitor"
    );
    assert_eq!(
        budget("api_key", "monthly", "shared", "hard"),
        "Key · Monthly · Shared · Hard"
    );
    let short = |scope: &str, interval: &str| {
        english(
            Text::message("usage-aixy-budget-short")
                .arg("scope", scope)
                .arg("interval", interval),
        )
    };
    assert_eq!(short("project", "monthly"), "Project monthly");
    assert_eq!(short("organization", "daily"), "Organization daily");
    assert_eq!(short("team", "weekly"), "Team weekly");
    assert_eq!(short("user", "lifetime"), "User lifetime");
    assert_eq!(short("api_key", "weekly"), "Key weekly");
    assert_eq!(
        english(
            Text::message("usage-aixy-budget-left")
                .arg("left", "$60.00")
                .arg("limit", "$100.00")
                .arg("spent", "$40.00")
        ),
        "$60.00 of $100.00 left, $40.00 spent"
    );
    assert_eq!(
        english(
            Text::message("usage-aixy-budget-reserved")
                .arg("left", "$70.00")
                .arg("limit", "$100.00")
                .arg("spent", "$20.00")
                .arg("reserved", "$10.00")
        ),
        "$70.00 of $100.00 left, $20.00 spent, $10.00 reserved"
    );
    assert_eq!(
        english(
            Text::message("usage-aixy-coverage")
                .arg("attributed", "10")
                .arg("requests", "1,200")
                .arg("partial", "2")
        ),
        "10 / 1,200 requests, 2 partial"
    );
    for (id, words) in [
        ("usage-aixy-key-title", "Aixy key"),
        ("usage-aixy-week", "Last 7 days · this key"),
        ("usage-aixy-attributed-week", "Last 7 days, attributed"),
        ("usage-aixy-unavailable", "Unavailable"),
        ("usage-aixy-unknown", "Unknown"),
    ] {
        assert_eq!(english(Text::message(id)), words);
    }
}

#[test]
fn alibaba_reads_as_it_did() {
    let window = |kind: &str| english(Text::message("usage-alibaba-window").arg("kind", kind));
    assert_eq!(window("session"), "Session");
    assert_eq!(window("weekly"), "Weekly");
    assert_eq!(window("monthly"), "Monthly");
    assert_eq!(
        english(Text::message("usage-alibaba-active-unreported")),
        "Active, quota not reported"
    );
}

#[test]
fn alibabatokenplan_reads_as_it_did() {
    let window =
        |kind: &str| english(Text::message("usage-alibabatokenplan-window").arg("kind", kind));
    assert_eq!(window("session"), "Session");
    assert_eq!(window("weekly"), "Weekly");
    assert_eq!(window("monthly"), "Monthly");
    assert_eq!(
        english(
            Text::message("usage-alibabatokenplan-credits-of")
                .arg("used", "150")
                .arg("total", "5,000")
        ),
        "150 / 5,000 credits"
    );
    assert_eq!(
        english(Text::message("usage-alibabatokenplan-active-plans")),
        "Active token plans"
    );
}

#[test]
fn amp_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-amp-hours").arg("hours", "732")),
        "732h"
    );
    assert_eq!(english(Text::message("usage-amp-under-an-hour")), "< 1h");
    assert_eq!(
        english(Text::message("usage-amp-workspace").arg("name", "meow")),
        "Workspace meow"
    );
    assert_eq!(
        english(Text::message("usage-amp-orb-hours")),
        "Orb (a1.small hours)"
    );
}

#[test]
fn antigravity_reads_as_it_did() {
    let window = |group: Text, cadence: &str, name: &str| {
        english(
            Text::message("usage-antigravity-window")
                .arg("group", group)
                .arg("cadence", cadence)
                .arg("name", name),
        )
    };
    assert_eq!(
        window(Text::service("Gemini"), "session", ""),
        "Gemini 5-hour"
    );
    assert_eq!(
        window(Text::service("Claude/GPT"), "weekly", ""),
        "Claude/GPT weekly"
    );
    assert_eq!(
        window(Text::message("usage-antigravity-quota"), "other", "Images"),
        "Quota Images"
    );
}

#[test]
fn atlascloud_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-atlascloud-available-balance")),
        "Available balance"
    );
}

#[test]
fn augment_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-augment-credits-per-month").arg("amount", "450,000")),
        "450,000 credits/month"
    );
    assert_eq!(
        english(Text::message("usage-augment-credits-left")),
        "Credits left"
    );
}

#[test]
fn azureopenai_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-azureopenai-deployment-plan").arg("deployment", "chat-prod")),
        "Deployment: chat-prod"
    );
    assert_eq!(
        english(Text::message("usage-azureopenai-reachable")),
        "Deployment reachable"
    );
}

#[test]
fn bedrock_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-bedrock-month-to-date")),
        "Month to date"
    );
    assert_eq!(
        english(Text::message("usage-bedrock-spend-by-service")),
        "Spend by service"
    );
}

#[test]
fn bifrost_reads_as_it_did() {
    let tokens = || Text::message("usage-bifrost-tokens");
    let provider = || Text::message("usage-bifrost-provider-scope").arg("name", "openai");
    let pool = Text::message("usage-bifrost-sourced-limit")
        .arg("source", "Team pool")
        .arg("limit", Text::message("usage-bifrost-requests"));
    assert_eq!(english(pool.clone()), "Team pool Requests");
    assert_eq!(
        english(
            Text::message("usage-bifrost-scoped-limit")
                .arg("scope", provider())
                .arg("limit", tokens())
        ),
        "Provider openai Tokens"
    );
    assert_eq!(
        english(
            Text::message("usage-bifrost-scoped-limit")
                .arg(
                    "scope",
                    Text::message("usage-bifrost-model-scope").arg("name", "openai · gpt-4o")
                )
                .arg("limit", pool)
        ),
        "Model openai · gpt-4o Team pool Requests"
    );
    let scoped = Text::message("usage-bifrost-scoped-budget")
        .arg("scope", provider())
        .arg("budget", Text::message("usage-bifrost-budget"));
    assert_eq!(english(scoped.clone()), "Provider openai · Budget");
    for (period, words) in [
        ("hourly", "Hourly"),
        ("daily", "Daily"),
        ("weekly", "Weekly"),
        ("monthly", "Monthly"),
        ("quarterly", "Quarterly"),
        ("yearly", "Yearly"),
    ] {
        assert_eq!(
            english(Text::message("usage-bifrost-period").arg("period", period)),
            words
        );
        assert_eq!(
            english(
                Text::message("usage-bifrost-periodic")
                    .arg("budget", "Engineering")
                    .arg("period", period)
            ),
            format!("Engineering · {words}")
        );
        assert_eq!(
            english(
                Text::message("usage-bifrost-amount-period")
                    .arg("amount", "$42.17 / $125.00")
                    .arg("period", period)
            ),
            format!("$42.17 / $125.00 · {words}")
        );
    }
    assert_eq!(
        english(
            Text::message("usage-bifrost-periodic")
                .arg("budget", scoped)
                .arg("period", "monthly")
        ),
        "Provider openai · Budget · Monthly"
    );
    assert_eq!(
        english(
            Text::message("usage-bifrost-budget-id")
                .arg("budget", Text::message("usage-bifrost-budget"))
                .arg("id", "year")
        ),
        "Budget year"
    );
    assert_eq!(
        english(
            Text::message("usage-bifrost-model-cost")
                .arg("cost", "$5.00")
                .arg("tokens", "1.2M")
        ),
        "$5.00 · 1.2M tokens"
    );
    assert_eq!(
        english(Text::message("usage-bifrost-other-models")),
        "Other models"
    );
}

#[test]
fn chutes_reads_as_it_did() {
    let amounts = |unit: Text| {
        english(
            Text::message("usage-chutes-amounts")
                .arg("used", "250")
                .arg("limit", "1000")
                .arg("unit", unit),
        )
    };
    assert_eq!(
        amounts(Text::message("usage-chutes-credits")),
        "250/1000 credits"
    );
    assert_eq!(amounts(Text::service("requests")), "250/1000 requests");
    assert_eq!(english(Text::message("usage-chutes-rolling")), "4-hour");
    assert_eq!(
        english(Text::message("usage-chutes-no-subscription")),
        "No active subscription"
    );
    assert_eq!(
        english(Text::message("usage-chutes-no-usage")),
        "No usage data"
    );
}

#[test]
fn claude_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-claude-spent-of")
                .arg("used", "12.34 EUR")
                .arg("limit", "50.00 EUR")
        ),
        "12.34 EUR of 50.00 EUR"
    );
    assert_eq!(
        english(Text::message("usage-claude-week-by-surface")),
        "This week by surface"
    );
    assert_eq!(english(Text::message("usage-claude-off")), "Off");
}

#[test]
fn clawrouter_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-clawrouter-routed-count").arg("count", 2)),
        "2 routed providers"
    );
    assert_eq!(
        english(
            Text::message("usage-clawrouter-request-counts")
                .arg("requests", "1,200")
                .arg("succeeded", "1,190")
                .arg("failed", "10")
        ),
        "1,200 · 1,190 succeeded · 10 failed"
    );
    assert_eq!(
        english(
            Text::message("usage-clawrouter-token-counts")
                .arg("tokens", "63,000")
                .arg("input", "42,000")
                .arg("output", "21,000")
        ),
        "63,000 · 42,000 input · 21,000 output"
    );
    assert_eq!(
        english(
            Text::message("usage-clawrouter-budget-remaining")
                .arg("amount", "$0.006000 / $25.00")
                .arg("remaining", "$24.994000")
        ),
        "$0.006000 / $25.00 · $24.994000 remaining"
    );
    assert_eq!(
        english(
            Text::message("usage-clawrouter-provider-usage")
                .arg("requests", "4")
                .arg("cost", "$0.004000")
                .arg("tokens", "42,000")
        ),
        "4 requests · $0.004000 · 42,000 tokens"
    );
    assert_eq!(
        english(Text::message("usage-clawrouter-managed-budget")),
        "Managed monthly budget"
    );
}

#[test]
fn codebuff_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-codebuff-auto-top-up")),
        "Auto top-up"
    );
    assert_eq!(english(Text::message("usage-codebuff-on")), "On");
}

#[test]
fn coderabbit_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-coderabbit-your-reviews")),
        "Your reviews this period"
    );
    assert_eq!(
        english(Text::message("usage-coderabbit-period-resets")),
        "Period resets"
    );
}

#[test]
fn codex_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-codex-code-review")),
        "Code review"
    );
    assert_eq!(
        english(Text::message("usage-codex-reset-credits")),
        "Limit reset credits"
    );
}

#[test]
fn commandcode_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-commandcode-monthly-left")),
        "Monthly credits left"
    );
    assert_eq!(
        english(Text::message("usage-commandcode-purchased")),
        "Purchased credits"
    );
}
