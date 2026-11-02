# Chutes's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-chutes-setting-api-key = A Chutes API key (cpk_…), created as described at https://chutes.ai/docs/getting-started/authentication.
usage-chutes-setting-base-url = Optional HTTPS management API URL. Defaults to https://api.chutes.ai.

## The report: the subscription's rolling and monthly quotas, or the
## account's per-chute quotas. Plan names, quota names, and units Chutes
## names stay as it sends them.

# The rolling quota, which refills over four hours.
usage-chutes-rolling = 4-hour
usage-chutes-monthly = Monthly
# A quota Chutes gives no name.
usage-chutes-quota = Quota
# The section listing each quota's amounts.
usage-chutes-quota-title = Quota
# A quota's use: $used of $limit, plain numbers such as 250 and 1000, in
# $unit, which is Chutes' own unit (such as requests) or
# usage-chutes-credits.
usage-chutes-amounts = { $used }/{ $limit } { $unit }
# The unit of a quota Chutes gives none.
usage-chutes-credits = credits
# The plan line of an account without a subscription.
usage-chutes-no-subscription = No active subscription
# The plan line when Chutes reports no quotas at all.
usage-chutes-no-usage = No usage data
