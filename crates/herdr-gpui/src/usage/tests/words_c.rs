//! The words a third batch of providers' reports name, read in English as
//! each provider used to write them.
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
fn kiro_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-kiro-credits-left")),
        "Credits left"
    );
    assert_eq!(
        english(Text::message("usage-kiro-bonus-credits")),
        "Bonus credits"
    );
    assert_eq!(english(Text::message("usage-kiro-overages")), "Overages");
    assert_eq!(
        english(Text::message("usage-kiro-overage-credits-used")),
        "Overage credits used"
    );
    assert_eq!(
        english(Text::message("usage-kiro-estimated-overage-cost")),
        "Estimated overage cost"
    );
}

#[test]
fn litellm_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-litellm-personal-budget")),
        "Personal budget"
    );
    assert_eq!(
        english(Text::message("usage-litellm-team-budget")),
        "Team budget"
    );
    assert_eq!(
        english(Text::message("usage-litellm-team-named-budget").arg("team", "ai")),
        "Team ai budget"
    );
    assert_eq!(
        english(Text::message("usage-litellm-personal-spend")),
        "Personal spend"
    );
    assert_eq!(
        english(Text::message("usage-litellm-team-spend")),
        "Team spend"
    );
    assert_eq!(
        english(Text::message("usage-litellm-team-named-spend").arg("team", "ai")),
        "Team ai spend"
    );
    assert_eq!(
        english(Text::message("usage-litellm-key-spend-only")),
        "Key spend only"
    );
    assert_eq!(
        english(Text::message("usage-litellm-user-spend-only")),
        "User spend only"
    );
    assert_eq!(english(Text::message("usage-litellm-api-key")), "API key");
    assert_eq!(
        english(
            Text::message("usage-litellm-period-range")
                .arg("start", "2026-09-01")
                .arg("end", "2026-09-02")
        ),
        "2026-09-01 – 2026-09-02 UTC"
    );
}

#[test]
fn llmman_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-llmman-memory-used")
                .arg("used", "10.0 GB")
                .arg("total", "40.0 GB")
        ),
        "10.0 GB of 40.0 GB"
    );
    assert_eq!(
        english(Text::message("usage-llmman-loaded-models")),
        "Loaded models"
    );
    assert_eq!(
        english(Text::message("usage-llmman-local-daemon")),
        "Local daemon"
    );
    assert_eq!(english(Text::message("usage-llmman-api-key")), "API key");
}

#[test]
fn llmproxy_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-llmproxy-active-keys")
                .arg("active", "3")
                .arg("keys", "4")
        ),
        "3/4 active keys"
    );
    assert_eq!(
        english(
            Text::message("usage-llmproxy-provider")
                .arg("requests", "120")
                .arg("tokens", "6,000")
        ),
        "120 req · 6,000 tok"
    );
    assert_eq!(
        english(
            Text::message("usage-llmproxy-provider-cost")
                .arg("requests", "120")
                .arg("tokens", "6,000")
                .arg("cost", "$12.50")
        ),
        "120 req · 6,000 tok · $12.50"
    );
    assert_eq!(
        english(Text::message("usage-llmproxy-approx-spend")),
        "Approx. spend"
    );
}

#[test]
fn longcat_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-longcat-token-quota")),
        "Token quota"
    );
    assert_eq!(
        english(Text::message("usage-longcat-fuel-pack-left")),
        "Fuel pack left"
    );
}

#[test]
fn manus_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-manus-left-of")
                .arg("left", "3,000")
                .arg("total", "4,000")
        ),
        "3,000 of 4,000"
    );
    assert_eq!(
        english(Text::message("usage-manus-monthly-credits")),
        "Monthly credits"
    );
    assert_eq!(
        english(Text::message("usage-manus-daily-refresh")),
        "Daily refresh"
    );
    assert_eq!(english(Text::message("usage-manus-refresh")), "Refresh");
}

#[test]
fn mimo_reads_as_it_did() {
    let used = |used: &str, limit: &str, count: i64| {
        english(
            Text::message("usage-mimo-credits-used")
                .arg("used", used)
                .arg("limit", limit)
                .arg("count", count),
        )
    };
    assert_eq!(
        used("10,100,158", "200,000,000", 200_000_000),
        "10,100,158 of 200,000,000 credits"
    );
    assert_eq!(used("0", "1", 1), "0 of 1 credit");
    assert_eq!(english(Text::message("usage-mimo-expired")), "Expired");
    assert_eq!(
        english(Text::message("usage-mimo-token-plan")),
        "Token plan"
    );
}

#[test]
fn minimax_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-minimax-daily")
                .arg("service", Text::message("usage-minimax-video"))
        ),
        "Video daily"
    );
    assert_eq!(
        english(
            Text::message("usage-minimax-hours")
                .arg("service", Text::message("usage-minimax-speech"))
                .arg("hours", 4)
        ),
        "Speech 4h"
    );
    assert_eq!(
        english(Text::message("usage-minimax-daily").arg("service", Text::service("custom-model"))),
        "custom-model daily"
    );
    assert_eq!(
        english(Text::message("usage-minimax-image-to-video")),
        "Image to video"
    );
    assert_eq!(
        english(Text::message("usage-minimax-text-to-video")),
        "Text to video"
    );
}

#[test]
fn mistral_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-mistral-allowance-used")
                .arg("used", "0.51 EUR")
                .arg("limit", "25.50 EUR")
        ),
        "0.51 EUR of 25.50 EUR used"
    );
    assert_eq!(
        english(Text::message("usage-mistral-api-spend")),
        "API spend this month"
    );
    assert_eq!(
        english(Text::message("usage-mistral-included-api")),
        "Included API"
    );
}

#[test]
fn moonshot_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-moonshot-in-deficit")),
        "In deficit"
    );
    assert_eq!(
        english(Text::message("usage-moonshot-available")),
        "Available"
    );
}

#[test]
fn muse_reads_as_it_did() {
    let hours = |hours: f64| english(Text::message("usage-muse-hours").arg("hours", hours));
    assert_eq!(hours(2.), "2 hours");
    assert_eq!(hours(2.5), "2.5 hours");
    assert_eq!(hours(1.), "1 hour");
    assert_eq!(english(Text::message("usage-muse-login")), "Muse login");
    assert_eq!(
        english(Text::message("usage-muse-payment-required")),
        "Payment method required; finish billing at https://dev.meta.ai"
    );
    assert_eq!(
        english(Text::message("usage-muse-no-subscription")),
        "No active subscription on this login"
    );
    assert_eq!(
        english(Text::message("usage-muse-no-quota")),
        "Quota not included in this login response"
    );
}

#[test]
fn neuralwatt_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-neuralwatt-energy-used")
                .arg("used", "13.90")
                .arg("included", "20")
        ),
        "13.90 / 20 kWh"
    );
    assert_eq!(
        english(Text::message("usage-neuralwatt-key-period").arg("period", "Monthly")),
        "Key Monthly"
    );
    assert_eq!(
        english(Text::message("usage-neuralwatt-key-allowance")),
        "Key Allowance"
    );
    let month = |requests: &str, count: i64| {
        english(
            Text::message("usage-neuralwatt-month-requests")
                .arg("cost", "$4.20")
                .arg("requests", requests)
                .arg("count", count),
        )
    };
    assert_eq!(month("1,234", 1234), "$4.20 · 1,234 requests");
    assert_eq!(month("1", 1), "$4.20 · 1 request");
    assert_eq!(
        english(
            Text::message("usage-neuralwatt-credits-of")
                .arg("used", "$17.32")
                .arg("total", "$50.00")
        ),
        "$17.32 of $50.00"
    );
    assert_eq!(
        english(Text::message("usage-neuralwatt-plan").arg("plan", "Standard")),
        "Standard plan"
    );
    assert_eq!(english(Text::message("usage-neuralwatt-yearly")), "Yearly");
}

#[test]
fn notion_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-notion-not-tracked")),
        "Not tracked; allowances apply to Business and Enterprise workspaces"
    );
    assert_eq!(english(Text::message("usage-notion-rolling")), "Rolling");
}

#[test]
fn nous_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-nous-subscription-credits")),
        "Subscription credits"
    );
    assert_eq!(
        english(Text::message("usage-nous-top-up-credits")),
        "Top-up credits"
    );
    assert_eq!(
        english(Text::message("usage-nous-subscription")),
        "Subscription"
    );
}

#[test]
fn ollama_reads_as_it_did() {
    assert_eq!(english(Text::message("usage-ollama-hourly")), "Hourly");
    assert_eq!(english(Text::message("usage-ollama-api-key")), "API key");
    assert_eq!(english(Text::message("usage-ollama-models")), "Models");
}

#[test]
fn openai_reads_as_it_did() {
    let spend = |days: i64| english(Text::message("usage-openai-spend-days").arg("days", days));
    assert_eq!(spend(1), "Spend today");
    assert_eq!(spend(30), "Spend, last 30 days");
    assert_eq!(spend(365), "Spend, last 365 days");
    assert_eq!(
        english(Text::message("usage-openai-admin-api-project").arg("project", "proj_abc")),
        "Admin API: proj_abc"
    );
    assert_eq!(
        english(Text::message("usage-openai-admin-api")),
        "Admin API"
    );
    assert_eq!(
        english(Text::message("usage-openai-unnamed-line-item")),
        "API"
    );
    assert_eq!(
        english(Text::message("usage-openai-unnamed-model")),
        "Responses and Chat Completions"
    );
    assert_eq!(
        english(Text::message("usage-openai-api-credits-left")),
        "API credits left"
    );
}

#[test]
fn opencode_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-opencode-zen-balance")),
        "Zen balance"
    );
    assert_eq!(
        english(Text::message("usage-opencode-monthly-limit-left")),
        "Monthly limit left"
    );
    assert_eq!(
        english(Text::message("usage-opencode-pay-as-you-go")),
        "Pay as you go"
    );
}

#[test]
fn opencodego_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-opencodego-zen-balance")),
        "Zen balance"
    );
    assert_eq!(english(Text::message("usage-opencodego-none")), "None");
}

#[test]
fn openrouter_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-openrouter-no-limit")),
        "No limit configured"
    );
    assert_eq!(
        english(Text::message("usage-openrouter-api-key-limit")),
        "API key limit"
    );
    assert_eq!(
        english(Text::message("usage-openrouter-last-30-days")),
        "Last 30 days"
    );
}
