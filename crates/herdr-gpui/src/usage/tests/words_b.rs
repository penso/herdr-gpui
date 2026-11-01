//! The words a report names, written by the app: messages with their
//! arguments read in English as the provider used to write them, and counts
//! take their unit's plural.
use super::super::text;
use herdr_usage::Text;

/// Drops Fluent's isolation marks around placeables.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn english(words: Text) -> String {
    plain(text::words(&words))
}

fn message(id: &'static str) -> String {
    english(Text::message(id))
}

#[test]
fn copilot_reads_as_it_did() {
    assert_eq!(message("usage-copilot-premium"), "Premium");
    assert_eq!(message("usage-copilot-chat"), "Chat");
    assert_eq!(message("usage-copilot-credits"), "Credits");
    assert_eq!(message("usage-copilot-credits-used"), "Credits used");
    assert_eq!(message("usage-copilot-quota"), "Quota");
    assert_eq!(message("usage-copilot-unlimited"), "Unlimited");
}

#[test]
fn cursor_reads_as_it_did() {
    assert_eq!(message("usage-cursor-auto"), "Auto + Composer");
    assert_eq!(message("usage-cursor-api"), "API");
    assert_eq!(
        english(
            Text::message("usage-cursor-of")
                .arg("used", "$15.00")
                .arg("limit", "$50.00")
        ),
        "$15.00 of $50.00"
    );
    let budget = |id: &'static str, budget: &str| english(Text::message(id).arg("budget", budget));
    assert_eq!(budget("usage-cursor-budget", "team"), "Team on-demand");
    assert_eq!(budget("usage-cursor-budget", "personal"), "On-demand");
    assert_eq!(
        budget("usage-cursor-budget-left", "team"),
        "Team on-demand left"
    );
    assert_eq!(
        budget("usage-cursor-budget-left", "personal"),
        "On-demand left"
    );
    assert_eq!(
        budget("usage-cursor-budget-spend", "team"),
        "Team on-demand spend"
    );
    assert_eq!(
        budget("usage-cursor-budget-spend", "personal"),
        "On-demand spend"
    );
    assert_eq!(message("usage-cursor-included"), "Included usage");
    assert_eq!(message("usage-cursor-spent"), "Spent this cycle");
}

#[test]
fn deepgram_reads_as_it_did() {
    assert_eq!(message("usage-deepgram-no-projects"), "No projects");
    assert_eq!(
        english(Text::message("usage-deepgram-project").arg("project", "Voice bot")),
        "Project: Voice bot"
    );
    assert_eq!(
        english(Text::message("usage-deepgram-projects").arg("count", 2)),
        "2 projects"
    );
    assert_eq!(
        english(
            Text::message("usage-deepgram-audio-hours")
                .arg("hours", "1.8")
                .arg("billable", "2.5")
        ),
        "1.8 hours · 2.5 billable"
    );
    assert_eq!(
        english(
            Text::message("usage-deepgram-period-range")
                .arg("start", "2026-05-20")
                .arg("end", "2026-06-30")
        ),
        "2026-05-20 to 2026-06-30"
    );
    assert_eq!(message("usage-deepgram-tts-characters"), "TTS characters");
}

#[test]
fn deepinfra_reads_as_it_did() {
    assert_eq!(message("usage-deepinfra-owed"), "Owed");
    assert_eq!(message("usage-deepinfra-available"), "Available");
    assert_eq!(
        message("usage-deepinfra-cycle-spend"),
        "Billing cycle spend"
    );
    assert_eq!(message("usage-deepinfra-month-spend"), "Spent this month");
    assert_eq!(message("usage-deepinfra-suspended"), "Suspended");
}

#[test]
fn deepseek_reads_as_it_did() {
    assert_eq!(
        message("usage-deepseek-add-credits"),
        "Add credits at platform.deepseek.com"
    );
    assert_eq!(
        message("usage-deepseek-unavailable"),
        "Balance unavailable for API calls"
    );
}

#[test]
fn devin_reads_as_it_did() {
    assert_eq!(message("usage-devin-extra-usage"), "Extra usage balance");
    assert_eq!(message("usage-devin-organization"), "Organization");
}

#[test]
fn devpass_reads_as_it_did() {
    assert_eq!(message("usage-devpass-pay-as-you-go"), "Pay as you go");
    assert_eq!(message("usage-devpass-api-key"), "API key (all time)");
    assert_eq!(message("usage-devpass-key-spend"), "Key usage (all time)");
    assert_eq!(message("usage-devpass-credits-left"), "Plan credits left");
}

#[test]
fn doubao_reads_as_it_did() {
    let window = |plan: &str, period: &str| {
        english(
            Text::message("usage-doubao-window")
                .arg("plan", plan)
                .arg("period", period),
        )
    };
    assert_eq!(window("agent", "session"), "Agent 5-hour");
    assert_eq!(window("team", "weekly"), "Team weekly");
    assert_eq!(window("agent-team", "monthly"), "Agent team monthly");
}

#[test]
fn elevenlabs_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-elevenlabs-of")
                .arg("used", "25,000")
                .arg("limit", "100,000")
        ),
        "25,000 of 100,000"
    );
    assert_eq!(
        message("usage-elevenlabs-professional-voices"),
        "Professional voices"
    );
}

#[test]
fn factory_reads_as_it_did() {
    assert_eq!(message("usage-factory-core-session"), "Core 5-hour");
    assert_eq!(message("usage-factory-core-monthly"), "Core monthly");
    assert_eq!(
        english(
            Text::message("usage-factory-plan-fallback")
                .arg("plan", "Factory Team - Team")
                .arg("preference", "credits")
        ),
        "Factory Team - Team - Fallback: credits"
    );
    assert_eq!(
        english(Text::message("usage-factory-fallback").arg("preference", "credits")),
        "Fallback: credits"
    );
}

#[test]
fn fireworks_reads_as_it_did() {
    assert_eq!(message("usage-fireworks-spend"), "Spend, last 30 days");
    assert_eq!(message("usage-fireworks-none"), "None");
}

#[test]
fn gemini_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-gemini-left").arg("percent", "40")),
        "40% left"
    );
    assert_eq!(message("usage-gemini-workspace"), "Workspace");
}

#[test]
fn gitkraken_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-gitkraken-credits")
                .arg("amount", "600")
                .arg("count", 600)
        ),
        "600 credits"
    );
    assert_eq!(
        english(
            Text::message("usage-gitkraken-used-of")
                .arg("used", "120")
                .arg("limit", "400")
                .arg("count", 400)
        ),
        "120 / 400 credits used"
    );
    assert_eq!(
        english(
            Text::message("usage-gitkraken-used-unlimited")
                .arg("used", "55.50")
                .arg("count", 55.5)
        ),
        "55.50 credits used · Unlimited"
    );
    assert_eq!(
        english(
            Text::message("usage-gitkraken-used-no-allowance")
                .arg("used", "1")
                .arg("count", 1)
        ),
        "1 credits used · No allowance"
    );
    assert_eq!(message("usage-gitkraken-rest"), "Rest of organization");
}

#[test]
fn grok_reads_as_it_did() {
    assert_eq!(message("usage-grok-not-reported"), "Not reported");
    assert_eq!(message("usage-grok-included-credits"), "Included credits");
}

#[test]
fn groq_reads_as_it_did() {
    assert_eq!(message("usage-groq-requests"), "Requests / min");
    assert_eq!(
        message("usage-groq-enterprise-only"),
        "Prometheus metrics need a GroqCloud Enterprise key"
    );
}

#[test]
fn helmcode_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-helmcode-tokens")
                .arg("used", "1,500,000")
                .arg("cap", "2,000,000")
                .arg("count", 2_000_000)
        ),
        "1,500,000 / 2,000,000 tokens"
    );
    assert_eq!(
        english(
            Text::message("usage-helmcode-tokens-credit")
                .arg("used", "250,000")
                .arg("cap", "1,000,000")
                .arg("count", 1_000_000)
                .arg("credit", "5,000")
        ),
        "250,000 / 1,000,000 tokens · 5,000 credit-funded"
    );
}

#[test]
fn huggingface_reads_as_it_did() {
    assert_eq!(
        english(Text::message("usage-huggingface-minutes").arg("minutes", "2.5")),
        "2.5 min"
    );
    assert_eq!(
        message("usage-huggingface-included"),
        "Included inference amount"
    );
}

#[test]
fn hyper_reads_as_it_did() {
    assert_eq!(message("usage-hyper-browser-session"), "Browser session");
    assert_eq!(message("usage-hyper-hypercredits"), "Hypercredits");
}

#[test]
fn ibmbob_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-ibmbob-used-of")
                .arg("used", "10")
                .arg("limit", "40")
                .arg("count", 40)
        ),
        "10 / 40 Bobcoins"
    );
    assert_eq!(
        english(
            Text::message("usage-ibmbob-used")
                .arg("used", "12.5")
                .arg("count", 12.5)
        ),
        "12.5 Bobcoins used"
    );
}

#[test]
fn jetbrains_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-jetbrains-refill-every")
                .arg("amount", "1,000,000")
                .arg("count", 1_000_000)
                .arg("days", 30)
        ),
        "1,000,000 credits every 30 days"
    );
    assert_eq!(
        english(
            Text::message("usage-jetbrains-refill-amount")
                .arg("amount", "500")
                .arg("count", 500)
        ),
        "500 credits"
    );
}

#[test]
fn kilo_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-kilo-pass-bonus")
                .arg("used", "$0.00")
                .arg("base", "$19.00")
                .arg("bonus", "$9.50")
        ),
        "$0.00 of $19.00 (+ $9.50 bonus)"
    );
    assert_eq!(
        english(
            Text::message("usage-kilo-pass-used")
                .arg("used", "$1.00")
                .arg("base", "$19.00")
        ),
        "$1.00 of $19.00"
    );
    assert_eq!(message("usage-kilo-off"), "Off");
    assert_eq!(message("usage-kilo-on"), "On");
}

#[test]
fn kimi_reads_as_it_did() {
    assert_eq!(
        english(
            Text::message("usage-kimi-of")
                .arg("used", "214")
                .arg("limit", "2,048")
        ),
        "214 of 2,048"
    );
    assert_eq!(message("usage-kimi-code-weekly"), "Code 7-day");
}
