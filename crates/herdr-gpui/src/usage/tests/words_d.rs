//! The words the reports of Perplexity through ZoomMate name read in
//! English as the providers used to write them.
use super::super::text;
use herdr_usage::Text;

/// Drops Fluent's isolation marks around placeables.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn english(words: Text) -> String {
    plain(text::words(&words))
}

/// A message without arguments, in English.
fn label(id: &'static str) -> String {
    english(Text::message(id))
}

#[test]
fn perplexity_reads_as_it_did() {
    assert_eq!(label("usage-perplexity-credits"), "Credits");
    assert_eq!(label("usage-perplexity-bonus-credits"), "Bonus credits");
    assert_eq!(
        label("usage-perplexity-purchased-credits"),
        "Purchased credits"
    );
    assert_eq!(label("usage-perplexity-bonus-expires"), "Bonus expires");
    assert_eq!(
        english(
            Text::message("usage-perplexity-used-of")
                .arg("used", "2,750")
                .arg("total", "10,000")
        ),
        "2,750 of 10,000"
    );
}

#[test]
fn poe_reads_as_it_did() {
    let points = |points: f64, amount: &str| {
        Text::message("usage-poe-point-count")
            .arg("points", points)
            .arg("amount", amount)
    };
    assert_eq!(english(points(1500., "1,500")), "1,500 points");
    assert_eq!(english(points(42.5, "42.5")), "42.5 points");
    assert_eq!(english(points(1., "1")), "1 point");
    let summary = |requests: u64| {
        Text::message("usage-poe-summary")
            .arg("points", points(300., "300"))
            .arg("requests", requests)
            .arg("count", requests.to_string())
    };
    assert_eq!(english(summary(2)), "300 points · 2 requests");
    assert_eq!(english(summary(1)), "300 points · 1 request");
    assert_eq!(
        english(
            Text::message("usage-poe-summary-cost")
                .arg("points", points(1000., "1,000"))
                .arg("requests", 1204)
                .arg("count", "1,204")
                .arg("cost", "$0.09")
        ),
        "1,000 points · 1,204 requests · $0.09"
    );
    assert_eq!(
        english(
            Text::message("usage-poe-model-points")
                .arg("model", "Claude-Sonnet")
                .arg("points", points(900., "900"))
        ),
        "Claude-Sonnet · 900 points"
    );
    assert_eq!(label("usage-poe-points"), "Points");
    assert_eq!(label("usage-poe-current-balance"), "Current balance");
    assert_eq!(label("usage-poe-today"), "Today");
    assert_eq!(label("usage-poe-last-7-days"), "Last 7 days");
    assert_eq!(label("usage-poe-last-30-days"), "Last 30 days");
    assert_eq!(label("usage-poe-top-model"), "Top model");
    assert_eq!(
        label("usage-poe-points-by-model"),
        "Points by model, last 30 days"
    );
}

#[test]
fn qoder_reads_as_it_did() {
    assert_eq!(label("usage-qoder-credits"), "Credits");
    assert_eq!(label("usage-qoder-credits-left"), "Credits left");
}

#[test]
fn raycast_reads_as_it_did() {
    assert_eq!(label("usage-raycast-credits"), "Credits");
    assert_eq!(label("usage-raycast-credits-left"), "Credits left");
    assert_eq!(label("usage-raycast-left"), "Left");
    assert_eq!(label("usage-raycast-total"), "Total");
    assert_eq!(label("usage-raycast-renews"), "Renews");
}

#[test]
fn replicate_reads_as_it_did() {
    assert_eq!(
        label("usage-replicate-spent-this-month"),
        "Spent this month"
    );
    assert_eq!(label("usage-replicate-credit-balance"), "Credit balance");
    assert_eq!(label("usage-replicate-account"), "Account");
    assert_eq!(label("usage-replicate-organization"), "Organization");
    assert_eq!(label("usage-replicate-user"), "User");
}

#[test]
fn sakana_reads_as_it_did() {
    assert_eq!(label("usage-sakana-credit-balance"), "Credit balance");
    assert_eq!(
        label("usage-sakana-pay-as-you-go-usage"),
        "Pay-as-you-go usage"
    );
    assert_eq!(label("usage-sakana-pay-as-you-go"), "Pay as you go");
    assert_eq!(label("usage-sakana-usage-period"), "Usage period");
}

#[test]
fn stepfun_reads_as_it_did() {
    assert_eq!(label("usage-stepfun-credit"), "Credit");
}

#[test]
fn sub2api_reads_as_it_did() {
    assert_eq!(label("usage-sub2api-quota"), "Quota");
    assert_eq!(
        english(Text::message("usage-sub2api-window-limit").arg("window", "30m")),
        "30m limit"
    );
    assert_eq!(label("usage-sub2api-today-requests"), "Today requests");
    assert_eq!(label("usage-sub2api-today-tokens"), "Today tokens");
    assert_eq!(
        label("usage-sub2api-all-time-requests"),
        "All time requests"
    );
    assert_eq!(label("usage-sub2api-all-time-tokens"), "All time tokens");
    assert_eq!(label("usage-sub2api-expires"), "Expires");
    assert_eq!(label("usage-sub2api-balance"), "Balance");
    assert_eq!(label("usage-sub2api-usage-summary"), "Usage summary");
}

#[test]
fn synthetic_reads_as_it_did() {
    assert_eq!(label("usage-synthetic-search-hourly"), "Search hourly");
    assert_eq!(
        label("usage-synthetic-weekly-credits-left"),
        "Weekly credits left"
    );
    assert_eq!(
        english(Text::message("usage-synthetic-quota").arg("number", 2)),
        "Quota 2"
    );
}

#[test]
fn t3chat_reads_as_it_did() {
    assert_eq!(label("usage-t3chat-overage"), "Overage");
    assert_eq!(label("usage-t3chat-plan"), "Plan");
    assert_eq!(label("usage-t3chat-usage-band"), "Usage band");
    assert_eq!(label("usage-t3chat-subscription"), "Subscription");
}

#[test]
fn typesafe_reads_as_it_did() {
    assert_eq!(label("usage-typesafe-balance"), "Balance");
    assert_eq!(label("usage-typesafe-spent"), "Spent");
    assert_eq!(
        english(Text::message("usage-typesafe-spent-cycle").arg("cycle", "September 2026")),
        "Spent (September 2026)"
    );
    assert_eq!(label("usage-typesafe-credits"), "Credits");
    assert_eq!(label("usage-typesafe-credit"), "Credit");
    assert_eq!(
        english(
            Text::message("usage-typesafe-credit-left")
                .arg("remaining", "4.98")
                .arg("amount", "5")
                .arg("expires", "Oct 19")
        ),
        "4.98 of 5, expires Oct 19"
    );
    assert_eq!(
        label("usage-typesafe-additional-credits"),
        "Additional credits"
    );
}

#[test]
fn v0_reads_as_it_did() {
    assert_eq!(label("usage-v0-api"), "v0 API");
    assert_eq!(label("usage-v0-rate-limit"), "Rate limit");
    assert_eq!(label("usage-v0-billing-remaining"), "Billing remaining");
    assert_eq!(label("usage-v0-on-demand-balance"), "On-demand balance");
    assert_eq!(
        label("usage-v0-rate-limit-remaining"),
        "Rate-limit remaining"
    );
    assert_eq!(label("usage-v0-billing-type"), "Billing type");
    assert_eq!(label("usage-v0-scope"), "Scope");
    assert_eq!(
        english(
            Text::message("usage-v0-remaining-of")
                .arg("remaining", "750")
                .arg("limit", "1,000")
        ),
        "750 of 1,000"
    );
    assert_eq!(
        english(Text::message("usage-v0-unavailable").arg("limit", "1,000")),
        "Unavailable (limit 1,000)"
    );
}

#[test]
fn venice_reads_as_it_did() {
    assert_eq!(label("usage-venice-diem-allocation"), "DIEM allocation");
    assert_eq!(label("usage-venice-status"), "Status");
    assert_eq!(label("usage-venice-api-calls"), "API calls");
    assert_eq!(
        label("usage-venice-unavailable-for-api-calls"),
        "Balance unavailable for API calls"
    );
    assert_eq!(
        label("usage-venice-subscription-credits"),
        "Subscription credits"
    );
    assert_eq!(label("usage-venice-total-credits"), "Total credits");
    assert_eq!(label("usage-venice-used-this-cycle"), "Used this cycle");
    assert_eq!(label("usage-venice-credits"), "Credits");
    assert_eq!(label("usage-venice-bank-cap"), "Bank cap");
    assert_eq!(label("usage-venice-next-refill"), "Next refill");
    let credits = |credits: f64, count: &str| {
        english(
            Text::message("usage-venice-credit-count")
                .arg("credits", credits)
                .arg("count", count),
        )
    };
    assert_eq!(credits(3000., "3,000"), "3,000 credits");
    assert_eq!(credits(1., "1"), "1 credit");
}

#[test]
fn vercel_reads_as_it_did() {
    assert_eq!(label("usage-vercel-available-balance"), "Available balance");
    assert_eq!(label("usage-vercel-lifetime-spend"), "Lifetime spend");
}

#[test]
fn vertexai_reads_as_it_did() {
    assert_eq!(label("usage-vertexai-peak-quota"), "Peak quota");
    assert_eq!(label("usage-vertexai-google-cloud"), "Google Cloud");
    assert_eq!(label("usage-vertexai-project"), "Project");
    assert_eq!(
        label("usage-vertexai-project-not-set"),
        "Not set: run `gcloud config set project PROJECT_ID`"
    );
}

#[test]
fn warp_reads_as_it_did() {
    assert_eq!(label("usage-warp-credits"), "Credits");
    assert_eq!(label("usage-warp-add-on-credits"), "Add-on credits");
    assert_eq!(label("usage-warp-monthly-credits"), "Monthly credits");
    assert_eq!(label("usage-warp-unlimited"), "Unlimited");
    assert_eq!(
        english(
            Text::message("usage-warp-used-of")
                .arg("used", "5")
                .arg("limit", "1,500")
        ),
        "5 of 1,500"
    );
    assert_eq!(label("usage-warp-next-add-on-expiry"), "Next add-on expiry");
    let expiring = |credits: i64, count: &str| {
        english(
            Text::message("usage-warp-credits-on")
                .arg("credits", credits)
                .arg("count", count)
                .arg("date", "Mar 1, 2026"),
        )
    };
    assert_eq!(expiring(10, "10"), "10 credits on Mar 1, 2026");
    assert_eq!(expiring(1, "1"), "1 credit on Mar 1, 2026");
}

#[test]
fn windsurf_reads_as_it_did() {
    assert_eq!(label("usage-windsurf-messages"), "Messages");
    assert_eq!(label("usage-windsurf-flow-actions"), "Flow actions");
    assert_eq!(label("usage-windsurf-plan"), "Plan");
    assert_eq!(label("usage-windsurf-renews"), "Renews");
}

#[test]
fn xai_reads_as_it_did() {
    assert_eq!(label("usage-xai-prepaid-balance"), "Prepaid balance");
    assert_eq!(label("usage-xai-management-api"), "Management API");
    assert_eq!(label("usage-xai-spend"), "Spend");
    assert_eq!(label("usage-xai-today"), "Today");
    assert_eq!(label("usage-xai-last-30-days"), "Last 30 days");
    assert_eq!(
        label("usage-xai-last-30-days-partial"),
        "Last 30 days (partial)"
    );
}

#[test]
fn xkiro_reads_as_it_did() {
    assert_eq!(label("usage-xkiro-free-tokens"), "Free tokens");
    assert_eq!(label("usage-xkiro-tokens-used-today"), "Tokens used today");
    assert_eq!(label("usage-xkiro-daily-allowance"), "Daily allowance");
    assert_eq!(label("usage-xkiro-no-cap-reported"), "No cap reported");
    assert_eq!(label("usage-xkiro-tokens-remaining"), "Tokens remaining");
    assert_eq!(label("usage-xkiro-daily-reset"), "Daily reset");
    assert_eq!(label("usage-xkiro-pay-as-you-go"), "Pay as you go");
    assert_eq!(label("usage-xkiro-wallet"), "Wallet");
}

#[test]
fn zai_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-zai-hours").arg("hours", 3_u64)),
        "3-hour"
    );
    assert_eq!(label("usage-zai-quota"), "Quota");
    assert_eq!(label("usage-zai-quota-details"), "Quota details");
    assert_eq!(label("usage-zai-credit-quota"), "Credit quota");
    assert_eq!(label("usage-zai-token-quota"), "Token quota");
    assert_eq!(
        label("usage-zai-session-credit-quota"),
        "Session credit quota"
    );
    assert_eq!(
        label("usage-zai-session-token-quota"),
        "Session token quota"
    );
    assert_eq!(label("usage-zai-mcp-quota"), "MCP quota");
    let detail = |percent: &str, counts: &str| {
        english(
            Text::message("usage-zai-detail")
                .arg("percent", percent)
                .arg("counts", counts)
                .arg("limit", "1000")
                .arg("remaining", "776"),
        )
    };
    assert_eq!(detail("9", "none"), "9% used");
    assert_eq!(
        detail("22.4", "both"),
        "22.4% used · 1000 limit · 776 remaining"
    );
    assert_eq!(detail("22.4", "limit"), "22.4% used · 1000 limit");
    assert_eq!(detail("22.4", "remaining"), "22.4% used · 776 remaining");
    assert_eq!(label("usage-zai-quota-rate"), "Quota rate");
    assert_eq!(label("usage-zai-peak"), "Peak");
    assert_eq!(label("usage-zai-off-peak"), "Off-peak");
    assert_eq!(label("usage-zai-account-balance"), "Account balance");
    assert_eq!(
        label("usage-zai-tokens-last-30-days"),
        "Tokens, last 30 days"
    );
    assert_eq!(label("usage-zai-unknown-model"), "Unknown");
}

#[test]
fn zed_reads_as_it_did() {
    assert_eq!(label("usage-zed-plan"), "Plan");
    assert_eq!(label("usage-zed-edit-predictions"), "Edit predictions");
    assert_eq!(
        english(
            Text::message("usage-zed-used-of")
                .arg("used", "30")
                .arg("limit", "120")
        ),
        "30 of 120"
    );
    assert_eq!(
        english(Text::message("usage-zed-used-unlimited").arg("used", "30")),
        "30 used · Unlimited"
    );
    assert_eq!(label("usage-zed-billing"), "Billing");
    assert_eq!(label("usage-zed-overdue-invoices"), "Overdue invoices");
    assert_eq!(label("usage-zed-token-spend"), "Token spend");
    assert_eq!(
        label("usage-zed-current-billing-period"),
        "Current billing period"
    );
    assert_eq!(label("usage-zed-spent"), "Spent");
    assert_eq!(label("usage-zed-spend-limit"), "Spend limit");
    assert_eq!(label("usage-zed-not-reported"), "Not reported");
    assert_eq!(label("usage-zed-remaining-budget"), "Remaining budget");
}

#[test]
fn zenmux_reads_as_it_did() {
    assert_eq!(label("usage-zenmux-quota"), "Quota");
    assert_eq!(label("usage-zenmux-session"), "Session");
    assert_eq!(label("usage-zenmux-weekly"), "Weekly");
    let flows = |used: &str, max: &str, flows: f64| {
        english(
            Text::message("usage-zenmux-flows")
                .arg("used", used)
                .arg("max", max)
                .arg("flows", flows),
        )
    };
    assert_eq!(flows("57.20", "800", 800.), "57.20 / 800 flows");
    assert_eq!(flows("0.50", "1", 1.), "0.50 / 1 flow");
    assert_eq!(
        english(Text::message("usage-zenmux-tier-plan").arg("tier", "Ultra")),
        "Ultra plan"
    );
    assert_eq!(
        english(
            Text::message("usage-zenmux-tier-plan-status")
                .arg("tier", "Ultra")
                .arg("status", "Overdue")
        ),
        "Ultra plan · Overdue"
    );
    assert_eq!(label("usage-zenmux-plan-expires"), "Plan expires");
    assert_eq!(label("usage-zenmux-payg-balance"), "PAYG balance");
}

#[test]
fn zoommate_reads_as_it_did() {
    assert_eq!(label("usage-zoommate-credits"), "Credits");
    assert_eq!(label("usage-zoommate-credits-used"), "Credits used");
    assert_eq!(label("usage-zoommate-credits-left"), "Credits left");
    assert_eq!(label("usage-zoommate-budget"), "Budget");
    assert_eq!(label("usage-zoommate-unlimited"), "Unlimited");
    assert_eq!(label("usage-zoommate-overage"), "Overage");
    assert_eq!(
        english(Text::message("usage-zoommate-overage-credits").arg("credits", "12.50")),
        "12.50 credits"
    );
    assert_eq!(label("usage-zoommate-quota"), "Quota");
    assert_eq!(label("usage-zoommate-exhausted"), "Exhausted");
}
