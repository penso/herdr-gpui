# ZenMux's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-zenmux-setting-api-key = A Management API key created at https://zenmux.ai/platform/management. Normal inference API keys are rejected by the usage endpoints.

## The report: the 5-hour and weekly flow quotas, the plan, and the
## pay-as-you-go balance. The plan tier and account status stay as ZenMux
## sends them, capitalized.

usage-zenmux-quota = Quota
usage-zenmux-session = Session
usage-zenmux-weekly = Weekly
# Flows used of a quota; $used and $max are numbers such as 57.20 and 800,
# and $flows is $max, to pick the plural by.
usage-zenmux-flows = { $flows ->
        [one] { $used } / { $max } flow
       *[other] { $used } / { $max } flows
    }
# The plan line; $tier is the plan's tier, such as Ultra.
usage-zenmux-tier-plan = { $tier } plan
# The same, with $status the account's status when it is not healthy, such
# as Overdue.
usage-zenmux-tier-plan-status = { $tier } plan · { $status }
# When the plan expires; the value is a date.
usage-zenmux-plan-expires = Plan expires
usage-zenmux-payg-balance = PAYG balance
