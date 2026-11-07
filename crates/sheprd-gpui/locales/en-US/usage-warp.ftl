# Warp's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-warp-setting-api-key = A Warp API key (wk-…). In Warp open your profile menu, then Settings → Platform → API Keys, and create one. See https://docs.warp.dev/reference/cli/api-keys.

## The report: the monthly credits and add-on credit grants.

usage-warp-credits = Credits
usage-warp-add-on-credits = Add-on credits
usage-warp-monthly-credits = Monthly credits
# The plan line, and the monthly credits, on an unlimited plan.
usage-warp-unlimited = Unlimited
# Monthly credits used; $used and $limit are grouped numbers, such as 5 and
# 1,500.
usage-warp-used-of = { $used } of { $limit }
usage-warp-next-add-on-expiry = Next add-on expiry
# The add-on credits that expire first; $credits is their number, $count it
# grouped, such as 1,200, and $date the day, such as Mar 1, 2026.
usage-warp-credits-on = { $credits ->
        [one] { $count } credit on { $date }
       *[other] { $count } credits on { $date }
    }
