//! Alibaba Cloud Model Studio / Bailian Token Plan usage, Team (credit pool)
//! or Personal/Solo (five-hour, weekly, and monthly windows) as `region`
//! selects. Sign-in sources, in CodexBar's order: the signed-in Bailian CLI
//! (`bl usage token-plan`, or `bl console call` for Personal plans), whose
//! JSON output holds usage only; then a console session cookie (the `cookie`
//! setting or `ALIBABA_TOKEN_PLAN_COOKIE`, or, when the provider is listed in
//! `[usage] show_providers`, the aliyun / alibabacloud cookies from Chrome or
//! Safari) for the OneConsole gateway.
//!
//! The gateway's `sec_token` comes from the `sec_token` setting or the
//! console's `/tool/user/info.json`; CodexBar also scrapes it from the
//! dashboard HTML, which the probe cannot do. Not ported: the
//! `ALIBABA_TOKEN_PLAN_HOST` / `_QUOTA_URL` test overrides, the `cna`
//! anonymous id and CSRF headers derived from individual cookies (the probe
//! keeps the cookie header opaque), and Firefox cookies.
//!
//! The OneConsole helpers here are shared with [`super::alibaba`] and
//! [`super::qwencloud`].

use crate::{
    Error, Result,
    usage::{
        model::{
            Account, Balance, Kind, MONTH, Provider, Report, SESSION, Section, Unit, WEEK, Window,
            group, title_case,
        },
        probe::{Part, Probe, Request, Secret},
        service::{Meta, Service, Setting, Timestamp},
        values,
    },
};
use serde_json::{Map, Value};
use std::time::{Duration, SystemTime};

// Shared with [`super::alibaba`], which reads the same gateway numbers.
pub(super) use crate::usage::values::number;

pub(super) const CHROME_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";
const SAFARI_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.3 Safari/605.1.15";
/// Console sessions live on the aliyun and alibabacloud passport domains.
pub(super) const DOMAINS: &[&str] = &["aliyun.com", "alibabacloud.com"];
const PERSONAL_PRODUCT: &str = "sfm_bailian";
const USAGE_API: &str = "zeldaHttp.apikeyMgr./tokenplan/personal/api/v2/usage";
const SUBSCRIPTION_API: &str = "zeldaHttp.apikeyMgr./tokenplan/personal/api/v2/subscription";
const QUOTA_CONFIG_API: &str = "zeldaHttp.apikeyMgr./tokenplan/personal/api/v2/quota-config";
/// The Personal gateway sometimes answers "Success" without the windows; an
/// immediate retry usually has them.
const USAGE_ATTEMPTS: usize = 3;
const CLI_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) struct Alibabatokenplan;

static META: Meta = Meta::new("alibabatokenplan", "Alibaba Token Plan")
    .icon("icons/providers/alibaba.svg")
    .dashboard("https://modelstudio.console.alibabacloud.com/ap-southeast-1/?tab=plan#/efm/subscription/token-plan")
    .status_page("https://status.aliyun.com")
    .settings(&[
        Setting::new(
            "region",
            &[],
            "Which Token Plan to read: \"intl\" (International Team, the default), \"cn\" \
             (China mainland Team), \"intl-personal\" or \"cn-personal\" (Personal/Solo).",
        ),
        Setting::new(
            "cookie",
            &["ALIBABA_TOKEN_PLAN_COOKIE"],
            "Only needed without a signed-in Bailian CLI (bl). Sign in to the Token Plan page \
             (https://modelstudio.console.alibabacloud.com, or https://bailian.console.aliyun.com \
             in China), open Developer Tools > Application > Cookies for that site, and copy \
             at least login_aliyunid_ticket, login_aliyunid_pk, login_aliyunid_csrf and cna. \
             Paste them as \"name=value; name2=value2\", or copy the whole Cookie header of the \
             data/api.json request from the Network tab.",
        ),
        Setting::new(
            "sec_token",
            &[],
            "Optional. The console's sec_token, when it cannot be read from \
             /tool/user/info.json: in Developer Tools > Network, the sec_token form field of \
             any data/api.json request on the Token Plan page.",
        ),
    ]);

impl Service for Alibabatokenplan {
    fn meta(&self) -> &'static Meta {
        &META
    }

    fn fetch(&self, probe: &mut Probe) -> Option<Result<Report>> {
        let region = probe
            .text_setting("region")
            .and_then(|raw| Region::parse(&raw))
            .unwrap_or(Region::Intl);
        let mut cli_answered = false;
        if let Ok(output) = probe.command("bl", &region.cli_args(), CLI_TIMEOUT)
            && output.success
        {
            if let Some(report) = parse_cli(&output.stdout) {
                return Some(Ok(report));
            }
            cli_answered = true;
        }
        let Some(cookie) = probe.cookies(DOMAINS, &[]) else {
            return cli_answered.then_some(Err(values::invalid()));
        };
        let token = sec_token(probe, &cookie, region.gateway());
        Some(if region.personal() {
            fetch_personal(probe, &cookie, token.as_ref(), region)
        } else {
            fetch_team(probe, &cookie, token.as_ref(), region)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    Intl,
    Cn,
    IntlPersonal,
    CnPersonal,
}

impl Region {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "intl" | "" => Some(Self::Intl),
            "cn" => Some(Self::Cn),
            "intl-personal" => Some(Self::IntlPersonal),
            "cn-personal" => Some(Self::CnPersonal),
            _ => None,
        }
    }

    fn personal(self) -> bool {
        matches!(self, Self::IntlPersonal | Self::CnPersonal)
    }

    fn china(self) -> bool {
        matches!(self, Self::Cn | Self::CnPersonal)
    }

    fn gateway(self) -> &'static str {
        if self.china() {
            "https://bailian.console.aliyun.com"
        } else {
            "https://modelstudio.console.alibabacloud.com"
        }
    }

    fn quota_base(self) -> &'static str {
        match self {
            Self::Intl | Self::Cn => self.gateway(),
            Self::IntlPersonal => "https://bailian-singapore-cs.alibabacloud.com",
            Self::CnPersonal => "https://bailian-cs.console.aliyun.com",
        }
    }

    fn dashboard(self) -> &'static str {
        match self {
            Self::Intl => {
                "https://modelstudio.console.alibabacloud.com/ap-southeast-1/?tab=plan#/efm/subscription/token-plan"
            }
            Self::Cn => {
                "https://bailian.console.aliyun.com/cn-beijing?tab=plan#/efm/subscription/token-plan"
            }
            Self::IntlPersonal => {
                "https://modelstudio.console.alibabacloud.com/ap-southeast-1/?tab=plan#/efm/subscription/token-plan/personal"
            }
            Self::CnPersonal => {
                "https://bailian.console.aliyun.com/cn-beijing?tab=plan#/efm/subscription/token-plan/personal"
            }
        }
    }

    fn region_id(self) -> &'static str {
        if self.china() {
            "cn-beijing"
        } else {
            "ap-southeast-1"
        }
    }

    fn product_code(self) -> &'static str {
        match self {
            Self::Intl => "sfm_tokenplanteams_dp_intl",
            Self::Cn => "sfm_tokenplanteams_dp_cn",
            Self::IntlPersonal => "sfm_tokenplansolo_public_intl",
            Self::CnPersonal => "sfm_tokenplansolo_public_cn",
        }
    }

    fn action(self) -> &'static str {
        if self.china() {
            "BroadScopeAspnGateway"
        } else {
            "IntlBroadScopeAspnGateway"
        }
    }

    /// Alibaba's live console contract, historical spelling included.
    fn console_site(self) -> &'static str {
        if self.china() {
            "BAILIAN_ALIYUN"
        } else {
            "MODELSTUDIO_ALBABACLOUD"
        }
    }

    fn cli_args(self) -> Vec<&'static str> {
        let mut args = if self.personal() {
            vec!["console", "call", "--api", USAGE_API, "--data", "{}"]
        } else {
            vec!["usage", "token-plan"]
        };
        args.extend([
            "--console-region",
            self.region_id(),
            "--console-site",
            if self.china() {
                "domestic"
            } else {
                "international"
            },
            "--output",
            "json",
        ]);
        args
    }
}

fn fetch_team(
    probe: &mut Probe,
    cookie: &Secret,
    token: Option<&Secret>,
    region: Region,
) -> Result<Report> {
    let url = format!(
        "{}/data/api.json?action=GetSubscriptionSummary&product=BssOpenAPI-V3&_tag=",
        region.quota_base()
    );
    let params = serde_json::json!({ "ProductCode": region.product_code() }).to_string();
    let body = form_body(
        &[
            ("product", "BssOpenAPI-V3"),
            ("action", "GetSubscriptionSummary"),
            ("params", &params),
            ("region", region.region_id()),
        ],
        token,
    );
    let request = gateway_request(
        url,
        cookie,
        region.gateway(),
        region.dashboard(),
        "*/*",
        body,
    );
    let body = probe.body(request)?;
    parse_team(&body)
}

fn fetch_personal(
    probe: &mut Probe,
    cookie: &Secret,
    token: Option<&Secret>,
    region: Region,
) -> Result<Report> {
    let mut call = |api: &str, data: Value| -> Result<Value> {
        let url = format!(
            "{}/data/api.json?action={}&product={PERSONAL_PRODUCT}&api={}&_v=undefined",
            region.quota_base(),
            region.action(),
            encode(api)
        );
        let mut data = data;
        if let Some(object) = data.as_object_mut() {
            object.insert(
                "cornerstoneParam".into(),
                cornerstone(region.dashboard(), region.console_site(), true),
            );
        }
        let params = serde_json::json!({ "Api": api, "V": "1.0", "Data": data }).to_string();
        let body = form_body(
            &[
                ("product", PERSONAL_PRODUCT),
                ("action", region.action()),
                ("region", region.region_id()),
                ("language", "en-US"),
                ("params", &params),
            ],
            token,
        );
        let request = gateway_request(
            url,
            cookie,
            region.gateway(),
            region.dashboard(),
            "application/json, text/plain, */*",
            body,
        );
        let body = probe.body(request)?;
        let value = expand(
            serde_json::from_str(&body).map_err(|error| Error::UsageJson(error.classify()))?,
        );
        check(&value)?;
        Ok(value)
    };
    let subscription = call(
        SUBSCRIPTION_API,
        serde_json::json!({ "commodityCode": region.product_code() }),
    )
    .ok();
    let config = call(QUOTA_CONFIG_API, serde_json::json!({})).ok();
    for _ in 0..USAGE_ATTEMPTS {
        let usage = call(USAGE_API, serde_json::json!({}))?;
        if let Some(found) = personal(
            &usage,
            subscription.as_ref(),
            config.as_ref(),
            "Personal",
            false,
        ) {
            return Ok(found.report(Provider(&Alibabatokenplan)));
        }
    }
    Err(values::invalid())
}

/// The Bailian CLI's JSON, which carries only the Personal-style ratios.
pub(crate) fn parse_cli(stdout: &str) -> Option<Report> {
    let value: Value = serde_json::from_str(stdout.trim()).ok()?;
    value.as_object()?;
    personal(&expand(value), None, None, "Token Plan", true)
        .map(|personal| personal.report(Provider(&Alibabatokenplan)))
}

const USED_KEYS: &[&str] = &[
    "usedQuota",
    "used_quota",
    "usedCredits",
    "usedCredit",
    "consumedCredits",
    "usage",
    "used",
    "usedAmount",
    "consumeAmount",
    "usedValue",
    "UsedValue",
    "consumedValue",
    "ConsumedValue",
];
const TOTAL_KEYS: &[&str] = &[
    "totalQuota",
    "total_quota",
    "totalCredits",
    "totalCredit",
    "quota",
    "creditLimit",
    "creditsTotal",
    "monthlyTotalQuota",
    "amount",
    "totalValue",
    "TotalValue",
    "cycleTotalValue",
    "CycleTotalValue",
];
const REMAINING_KEYS: &[&str] = &[
    "remainingQuota",
    "remainQuota",
    "remainingCredits",
    "remainingCredit",
    "availableCredits",
    "balance",
    "remaining",
    "availableAmount",
    "remainAmount",
    "totalSurplusValue",
    "TotalSurplusValue",
    "surplusValue",
    "SurplusValue",
    "cycleSurplusValue",
    "CycleSurplusValue",
];
const COUNT_KEYS: &[&str] = &[
    "totalCount",
    "TotalCount",
    "subscriptionTotalNumber",
    "SubscriptionTotalNumber",
];
const RESET_KEYS: &[&str] = &[
    "nextRefreshTime",
    "resetTime",
    "periodEndTime",
    "billingCycleEnd",
    "billCycleEndTime",
    "expireTime",
    "expirationTime",
    "endTime",
    "validEndTime",
    "instanceEndTime",
    "EndTime",
    "cycleEndTime",
    "CycleEndTime",
    "nearestExpireDate",
    "NearestExpireDate",
];
const PLAN_KEYS: &[&str] = &[
    "planName",
    "plan_name",
    "packageName",
    "package_name",
    "commodityName",
    "commodity_name",
    "specType",
    "SpecType",
    "instanceName",
    "instance_name",
    "displayName",
    "display_name",
    "ProductName",
    "productName",
    "name",
    "title",
    "planType",
    "plan_type",
];

/// A Team subscription summary: the credit pool used, left, and when the
/// nearest plan expires.
pub(crate) fn parse_team(body: &str) -> Result<Report> {
    let value =
        expand(serde_json::from_str(body).map_err(|error| Error::UsageJson(error.classify()))?);
    check(&value)?;
    team(&value)
        .map(|report| report_with(Provider(&Alibabatokenplan), report))
        .ok_or_else(values::invalid)
}

/// What a subscription summary says, before it is tied to a provider.
pub(super) struct Team {
    plan: Option<String>,
    used: Option<f64>,
    total: Option<f64>,
    remaining: Option<f64>,
    resets_at: Option<SystemTime>,
}

pub(super) fn team(value: &Value) -> Option<Team> {
    let quota_keys: Vec<&str> = [USED_KEYS, TOTAL_KEYS, REMAINING_KEYS].concat();
    let summary_keys: Vec<&str> = [USED_KEYS, TOTAL_KEYS, REMAINING_KEYS, COUNT_KEYS].concat();
    let has_any = |object: &Map<String, Value>, keys: &[&str]| {
        keys.iter().any(|key| object.contains_key(*key))
    };
    let data = find_value(
        value,
        &["Data", "data", "successResponse", "success_response"],
        true,
        Value::as_object,
    )
    .filter(|data| has_any(data, &summary_keys));
    let summary: Map<String, Value> = match data {
        Some(data) if has_any(data, &quota_keys) => data.clone(),
        // Some consoles nest the quota numbers in an `EquityList` entry while
        // the outer frame only counts subscriptions.
        Some(data) => {
            let wrapped = Value::Object(data.clone());
            find_object(&wrapped, &quota_keys)
                .cloned()
                .unwrap_or_else(|| data.clone())
        }
        None => find_object(value, &summary_keys)?.clone(),
    };
    let nested = Value::Object(summary.clone());
    let total = first(&summary, TOTAL_KEYS, number);
    let remaining = first(&summary, REMAINING_KEYS, number);
    let used = first(&summary, USED_KEYS, number).or_else(|| {
        total
            .zip(remaining)
            .map(|(total, remaining)| (total - remaining).max(0.))
    });
    let count = first(&summary, COUNT_KEYS, number);
    let resets_at = find_value(&nested, RESET_KEYS, true, date)
        .or_else(|| find_value(value, RESET_KEYS, true, date));
    let plan = find_value(&nested, PLAN_KEYS, true, string).or_else(|| {
        (count.is_some_and(|count| count > 0.) || total.is_some()).then(|| "TOKEN PLAN".to_owned())
    });
    if plan.is_none() && total.is_none() && used.is_none() && remaining.is_none() && count.is_none()
    {
        return None;
    }
    Some(Team {
        plan,
        used,
        total,
        remaining,
        resets_at,
    })
}

pub(super) fn report_with(provider: Provider, team: Team) -> Report {
    let percent = team.total.filter(|total| *total > 0.).and_then(|total| {
        let used = team
            .used
            .or_else(|| team.remaining.map(|left| total - left))?;
        Some(used.clamp(0., total) / total * 100.)
    });
    let windows = percent
        .map(|used| Window::new(Kind::Monthly, used, team.resets_at, Some(MONTH)))
        .into_iter()
        .collect();
    let credits = || Unit::Count("credits".into());
    let balance = match (
        team.total.filter(|total| *total > 0.),
        team.remaining,
        team.used,
    ) {
        (Some(total), Some(left), _) => {
            Some(Balance::new("Credits left", left.max(0.), credits()).out_of(total))
        }
        (Some(total), None, Some(used)) => {
            Some(Balance::new("Credits left", (total - used).max(0.), credits()).out_of(total))
        }
        (None, Some(left), _) => Some(Balance::new("Credits left", left.max(0.), credits())),
        _ => None,
    };
    let account = Account {
        email: None,
        plan: team.plan.filter(|plan| !plan.trim().is_empty()),
    };
    let mut report = Report::new(provider, account, windows).with_balances(balance);
    if percent.is_none() && report.balances.is_empty() {
        report = report.with_sections([Section::Facts {
            title: "Subscription".into(),
            facts: vec![("Active token plans".into(), "None".into())],
        }]);
    }
    report
}

/// Personal/Solo windows, with the plan tier and its credit limits when the
/// subscription and quota-config answers are at hand.
pub(super) struct Personal {
    windows: Vec<Window>,
    plan: Option<String>,
    totals: [Option<f64>; 3],
}

impl Personal {
    pub(super) fn report(self, provider: Provider) -> Report {
        let facts: Vec<(String, String)> = self
            .windows
            .iter()
            .zip(self.totals)
            .filter_map(|(window, total)| {
                let total = total.filter(|total| *total > 0.)?;
                let used = total * f64::from(window.used) / 100.;
                Some((
                    window.kind.title().to_owned(),
                    format!(
                        "{} / {} credits",
                        group(used.round() as i64),
                        group(total.round() as i64)
                    ),
                ))
            })
            .collect();
        let report = Report::new(
            provider,
            Account {
                email: None,
                plan: self.plan,
            },
            self.windows,
        );
        if facts.is_empty() {
            report
        } else {
            report.with_sections([Section::Facts {
                title: "Credits used".into(),
                facts,
            }])
        }
    }
}

/// `strict` is the CLI's contract: ratios must be JSON numbers within 0..=1,
/// resets millisecond numbers, and a reset only counts with its ratio.
pub(super) fn personal(
    usage: &Value,
    subscription: Option<&Value>,
    config: Option<&Value>,
    default_plan: &str,
    strict: bool,
) -> Option<Personal> {
    let usage = find_object(
        usage,
        &[
            "per5HourPercentage",
            "per1WeekPercentage",
            "per1MonthPercentage",
        ],
    )?;
    let ratio = |key: &str| {
        let value = usage.get(key)?;
        let ratio = if strict {
            value.as_f64().filter(|ratio| (0. ..=1.).contains(ratio))?
        } else {
            number(value)?
        };
        ratio.is_finite().then(|| ratio.clamp(0., 1.) * 100.)
    };
    let reset = |key: &str| {
        let value = usage.get(key)?;
        if strict {
            let millis = value
                .as_f64()
                .filter(|millis| millis.is_finite() && *millis > 0.)?;
            SystemTime::UNIX_EPOCH.checked_add(Duration::try_from_secs_f64(millis / 1000.).ok()?)
        } else {
            date(value)
        }
    };
    let slots = [
        (
            Kind::Session,
            "per5HourPercentage",
            "per5HourResetTime",
            SESSION,
        ),
        (
            Kind::Weekly,
            "per1WeekPercentage",
            "per1WeekResetTime",
            WEEK,
        ),
        (
            Kind::Monthly,
            "per1MonthPercentage",
            "per1MonthResetTime",
            MONTH,
        ),
    ];
    let code = subscription.and_then(plan_code);
    let limits = code
        .as_deref()
        .zip(config)
        .and_then(|(code, config)| quota_totals(config, code));
    let mut windows = Vec::new();
    let mut totals = [None; 3];
    for (kind, percent_key, reset_key, length) in slots {
        let Some(percent) = ratio(percent_key) else {
            continue;
        };
        let index = windows.len();
        let slot = match kind {
            Kind::Session => 0,
            Kind::Weekly => 1,
            _ => 2,
        };
        totals[index] = limits.and_then(|limits| limits[slot]);
        windows.push(Window::new(kind, percent, reset(reset_key), Some(length)));
    }
    if windows.is_empty() {
        return None;
    }
    let plan = code.map_or_else(
        || default_plan.to_owned(),
        |code| {
            if matches!(code.as_str(), "lite" | "standard" | "pro" | "max") {
                title_case(&code)
            } else {
                code
            }
        },
    );
    Some(Personal {
        windows,
        plan: Some(plan),
        totals,
    })
}

fn plan_code(subscription: &Value) -> Option<String> {
    const KEYS: &[&str] = &["specCode", "spec_code", "planName", "plan_name"];
    let plan = find_object(subscription, KEYS)?;
    KEYS.iter()
        .find_map(|key| string(plan.get(*key)?))
        .map(|code| code.to_lowercase())
}

fn quota_totals(config: &Value, code: &str) -> Option<[Option<f64>; 3]> {
    let quota = find_value(config, &[code], true, Value::as_object)?;
    let get = |keys: &[&str]| keys.iter().find_map(|key| number(quota.get(*key)?));
    let totals = [
        get(&["five_hour", "fiveHour"]),
        get(&["weekly"]),
        get(&["monthly"]),
    ];
    totals.iter().any(Option::is_some).then_some(totals)
}

// --- OneConsole helpers, shared with the Alibaba Coding Plan and Qwen Cloud.

/// Expands strings that hold JSON, as the gateway nests stringified frames.
pub(super) fn expand(value: Value) -> Value {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if (trimmed.starts_with('{') || trimmed.starts_with('['))
                && let Ok(inner) = serde_json::from_str::<Value>(trimmed)
            {
                return expand(inner);
            }
            Value::String(text)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(expand).collect()),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| (key, expand(value)))
                .collect(),
        ),
        other => other,
    }
}

/// The first object, each checked before its descendants, for which `pick`
/// finds something.
pub(super) fn first_match<'a, T>(
    value: &'a Value,
    arrays: bool,
    pick: &impl Fn(&'a Map<String, Value>) -> Option<T>,
) -> Option<T> {
    match value {
        Value::Object(object) => pick(object).or_else(|| {
            object
                .values()
                .find_map(|nested| first_match(nested, arrays, pick))
        }),
        Value::Array(items) if arrays => items
            .iter()
            .find_map(|nested| first_match(nested, arrays, pick)),
        _ => None,
    }
}

/// The first object holding any of `keys`.
pub(super) fn find_object<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    first_match(value, true, &|object: &'a Map<String, Value>| {
        keys.iter()
            .any(|key| object.contains_key(*key))
            .then_some(object)
    })
}

/// The first of `keys`, in order at each object, that `convert` accepts.
pub(super) fn find_value<'a, T>(
    value: &'a Value,
    keys: &[&str],
    arrays: bool,
    convert: impl Fn(&'a Value) -> Option<T>,
) -> Option<T> {
    first_match(value, arrays, &|object: &'a Map<String, Value>| {
        first(object, keys, &convert)
    })
}

pub(super) fn first<'a, T>(
    object: &'a Map<String, Value>,
    keys: &[&str],
    convert: impl Fn(&'a Value) -> Option<T>,
) -> Option<T> {
    keys.iter().find_map(|key| convert(object.get(*key)?))
}

pub(super) fn string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Epoch seconds or milliseconds, ISO 8601, or `yyyy-MM-dd[ HH:mm[:ss]]`.
pub(super) fn date(value: &Value) -> Option<SystemTime> {
    if let Some(number) = number(value).filter(|number| *number > 0.) {
        let seconds = if number >= 1e12 {
            number / 1000.
        } else {
            number
        };
        return SystemTime::UNIX_EPOCH.checked_add(Duration::try_from_secs_f64(seconds).ok()?);
    }
    let text = value.as_str()?.trim();
    Timestamp::Text(text.to_owned()).time().or_else(|| {
        let at = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M")
            .ok()
            .or_else(|| {
                chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                    .ok()?
                    .and_hms_opt(0, 0, 0)
            })?;
        let seconds = u64::try_from(at.and_utc().timestamp()).ok()?;
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
    })
}

/// Maps the gateway's error envelopes: login and token failures are a
/// rejected session, other failures keep their status when it is an HTTP one.
pub(super) fn check(value: &Value) -> Result<()> {
    let text = |keys: &[&str], within: &Value| find_value(within, keys, true, string);
    let flag = |value: Option<&Value>| match value {
        Some(Value::Bool(flag)) => Some(*flag),
        Some(Value::String(text)) => match text.to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    };
    let failing = first_match(value, true, &|object: &Map<String, Value>| {
        (flag(object.get("success")) == Some(false)
            || flag(object.get("Success")) == Some(false)
            || flag(object.get("successResponse")) == Some(false))
        .then(|| Value::Object(object.clone()))
    });
    let status = find_value(
        value,
        &["statusCode", "status_code", "code"],
        true,
        |value| number(value).filter(|code| code.fract() == 0.),
    );
    let code = text(
        &["errorCode", "Code", "code", "status", "statusCode"],
        value,
    );
    let message = text(
        &["errorMsg", "Message", "message", "msg", "statusMessage"],
        value,
    );
    let combined = [
        failing
            .as_ref()
            .and_then(|frame| text(&["errorCode", "Code", "code"], frame)),
        failing
            .as_ref()
            .and_then(|frame| text(&["errorMsg", "Message", "message", "msg"], frame)),
        code,
        message,
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
    .to_lowercase();
    let login = [
        "needlogin",
        "login",
        "postonlyortokenerror",
        "tokenerror",
        "request has expired",
        "refresh page",
        "请求已经过期",
    ]
    .iter()
    .any(|marker| combined.contains(marker));
    let unauthorized = !combined.contains("workspace.notauthori")
        && [
            "notauthorised",
            "notauthorized",
            "not authorised",
            "not authorized",
            "unauthorised",
            "unauthorized",
            "access denied",
            "forbidden",
        ]
        .iter()
        .any(|marker| combined.contains(marker));
    let bad_status = status.filter(|code| *code != 0. && *code != 200.);
    if login || unauthorized || bad_status.is_some_and(|code| code == 401. || code == 403.) {
        return Err(Error::UsageRejected);
    }
    if failing.is_some() || bad_status.is_some() {
        return Err(
            match bad_status.and_then(|code| u16::try_from(code as i64).ok()) {
                Some(code) if (100..=599).contains(&code) => Error::UsageStatus(code),
                _ => values::invalid(),
            },
        );
    }
    Ok(())
}

pub(super) fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// A form body; the security token, when there is one, is spliced in where
/// it lives. It is sent as read, since console tokens are URL-safe.
pub(super) fn form_body(fields: &[(&str, &str)], token: Option<&Secret>) -> Vec<Part> {
    let mut text = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish();
    match token {
        Some(token) => {
            text.push_str("&sec_token=");
            vec![Part::Text(text), Part::Secret(token.clone())]
        }
        None => vec![Part::Text(text)],
    }
}

pub(super) fn gateway_request(
    url: String,
    cookie: &Secret,
    origin: &str,
    referer: &str,
    accept: &str,
    body: Vec<Part>,
) -> Request {
    Request::post(url)
        .cookie(cookie)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", accept)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("User-Agent", CHROME_AGENT)
        .header("Origin", origin)
        .header("Referer", referer)
        .body(body)
}

/// The OneConsole envelope every gateway API call carries.
pub(super) fn cornerstone(dashboard: &str, site: &str, switch_user: bool) -> Value {
    let domain = url::Url::parse(dashboard)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default();
    let mut envelope = serde_json::json!({
        "feTraceId": uuid::Uuid::new_v4().to_string(),
        "feURL": dashboard,
        "protocol": "V2",
        "console": "ONE_CONSOLE",
        "productCode": "p_efm",
        "domain": domain,
        "consoleSite": site,
        "userNickName": "",
        "userPrincipalName": "",
        "xsp_lang": "en-US",
    });
    if switch_user && let Some(object) = envelope.as_object_mut() {
        object.insert("switchUserType".into(), Value::from(3));
    }
    envelope
}

/// The `sec_token` setting, else the console's user-info answer. None when
/// neither has one; some gateways still accept the request without it.
pub(super) fn sec_token(probe: &mut Probe, cookie: &Secret, gateway: &str) -> Option<Secret> {
    if let Some(token) = probe.setting("sec_token") {
        return Some(token);
    }
    let request = Request::get(format!("{gateway}/tool/user/info.json"))
        .cookie(cookie)
        .header("Accept", "application/json, text/plain, */*")
        .header("Referer", format!("{gateway}/"))
        .header("User-Agent", SAFARI_AGENT);
    probe.exchange(request, &["data", "secToken"]).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn parses_team_summary() {
        let report = parse_team(
            r#"{"Success": true, "Data": {"TotalCount": 1, "TotalValue": 1000,
                "TotalSurplusValue": 875, "NearestExpireDate": 1701000000000}, "Code": "200"}"#,
        )
        .unwrap();
        assert_eq!(report.account.plan.as_deref(), Some("TOKEN PLAN"));
        assert_eq!(report.windows[0].kind, Kind::Monthly);
        assert_eq!(report.windows[0].used, 12.5);
        assert_eq!(report.windows[0].resets_at, Some(at(1_701_000_000)));
        assert_eq!(report.balances[0].amount, 875.);
        assert_eq!(report.balances[0].total, Some(1000.));
    }

    #[test]
    fn parses_stringified_team_summary() {
        let body = serde_json::json!({"successResponse": {"body":
            r#"{"success": true, "data": {"totalCount": 1, "totalSurplusValue": 750, "totalValue": 1000}}"#}})
        .to_string();
        let report = parse_team(&body).unwrap();
        assert_eq!(report.windows[0].used, 25.);
    }

    #[test]
    fn empty_team_summary_stays_visible() {
        let report = parse_team(r#"{"Success": true, "Data": {"TotalCount": 0}}"#).unwrap();
        assert!(report.windows.is_empty());
        assert_eq!(report.account.plan, None);
        assert_eq!(report.sections.len(), 1);
    }

    #[test]
    fn parses_personal_usage_with_tier_limits() {
        let usage = expand(serde_json::json!({
            "data": {"DataV2": {"data": r#"{"code":0,"data":{"per5HourPercentage":0.03,
                "per5HourResetTime":1700003600000,"per1WeekPercentage":0.01,
                "per1WeekResetTime":1700086400000},"success":true}"#}},
            "httpStatusCode": 200
        }));
        let subscription = serde_json::json!({"data":{"specCode":"standard","status":"VALID"}});
        let config = serde_json::json!({"data":{"lite":{"five_hour":1000,"weekly":10000},
            "standard":{"five_hour":5000,"weekly":50000}}});
        check(&usage).unwrap();
        let report = personal(
            &usage,
            Some(&subscription),
            Some(&config),
            "Personal",
            false,
        )
        .unwrap()
        .report(Provider(&Alibabatokenplan));
        assert_eq!(report.account.plan.as_deref(), Some("Standard"));
        assert_eq!(report.windows[0].kind, Kind::Session);
        assert!((report.windows[0].used - 3.).abs() < 1e-4);
        assert_eq!(report.windows[0].resets_at, Some(at(1_700_003_600)));
        assert_eq!(report.windows[1].kind, Kind::Weekly);
        assert!((report.windows[1].used - 1.).abs() < 1e-4);
        let Section::Facts { facts, .. } = &report.sections[0] else {
            panic!("expected credit facts");
        };
        assert_eq!(facts[0], ("Session".into(), "150 / 5,000 credits".into()));
    }

    #[test]
    fn cli_requires_numeric_ratios() {
        let report = parse_cli(
            r#"{"per5HourPercentage":0.5,"per5HourResetTime":1700003600000,
                "per1MonthPercentage":0.2,"per1MonthResetTime":1702000000000}"#,
        )
        .unwrap();
        assert_eq!(report.account.plan.as_deref(), Some("Token Plan"));
        assert_eq!(report.windows.len(), 2);
        assert_eq!(report.windows[1].kind, Kind::Monthly);
        assert!(parse_cli(r#"{"per5HourPercentage":"0.5"}"#).is_none());
        assert!(parse_cli("not json").is_none());
    }

    #[test]
    fn maps_gateway_errors() {
        let login = serde_json::json!({"code":"ConsoleNeedLogin","message":"please login"});
        assert!(matches!(check(&login), Err(Error::UsageRejected)));
        let failed = serde_json::json!({"successResponse":true,"data":{"success":false,
            "errorCode":"Throttling","errorMsg":"busy"}});
        assert!(matches!(check(&failed), Err(Error::UsageJson(_))));
        let status = serde_json::json!({"statusCode":500,"message":"oops"});
        assert!(matches!(check(&status), Err(Error::UsageStatus(500))));
        let workspace = serde_json::json!({"success":false,
            "code":"BailianGateway.Workspace.NotAuthorised"});
        assert!(matches!(check(&workspace), Err(Error::UsageJson(_))));
        assert!(check(&serde_json::json!({"code":"200","data":{}})).is_ok());
    }

    #[test]
    fn form_body_splices_the_token_last() {
        let token = Secret::from(secrecy::SecretString::from("tok123".to_owned()));
        let parts = form_body(
            &[("params", r#"{"a":"b c"}"#), ("region", "cn-beijing")],
            Some(&token),
        );
        let Part::Text(text) = &parts[0] else {
            panic!("expected text");
        };
        assert_eq!(
            text,
            "params=%7B%22a%22%3A%22b+c%22%7D&region=cn-beijing&sec_token="
        );
        assert!(matches!(parts[1], Part::Secret(_)));
    }

    #[test]
    fn reads_console_dates() {
        assert_eq!(
            date(&Value::from(1_700_000_000_000_u64)),
            Some(at(1_700_000_000))
        );
        assert_eq!(date(&Value::from("2023-11-14")), Some(at(1_699_920_000)));
        assert_eq!(
            date(&Value::from("2023-11-14 22:13")),
            Some(at(1_700_000_000 - 20))
        );
    }
}
