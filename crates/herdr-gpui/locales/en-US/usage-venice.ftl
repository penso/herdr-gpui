# Venice's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-venice-setting-api-key = A Venice API key from https://venice.ai/settings/api. It reads the USD and DIEM API balance.
usage-venice-setting-cookie = Used when no API key is set, for subscription credits. Sign in to https://venice.ai, open Developer Tools > Application > Cookies for https://venice.ai, and copy the __venice-auth.session-token cookie (or every __venice-auth.session-token.N chunk when it is split). Paste them as "name=value; name2=value2".

## The report: the USD and DIEM balances, and a web session's subscription
## credits. Balances named USD and DIEM, and the user type (such as pro),
## stay as Venice names them.

usage-venice-diem-allocation = DIEM allocation
usage-venice-status = Status
usage-venice-api-calls = API calls
usage-venice-unavailable-for-api-calls = Balance unavailable for API calls
usage-venice-subscription-credits = Subscription credits
usage-venice-total-credits = Total credits
usage-venice-used-this-cycle = Used this cycle
usage-venice-credits = Credits
# The most credits the plan can bank.
usage-venice-bank-cap = Bank cap
# A number of credits; $credits is the number, and $count it grouped, such
# as 3,000.
usage-venice-credit-count = { $credits ->
        [one] { $count } credit
       *[other] { $count } credits
    }
# When the subscription credits refill; the value is a date.
usage-venice-next-refill = Next refill
