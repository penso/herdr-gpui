# ClawRouter's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-clawrouter-setting-api-key = A ClawRouter policy API key, created at https://clawrouter.openclaw.ai/dashboard/access.
usage-clawrouter-setting-base-url = Optional HTTPS service root or /v1 URL of another ClawRouter deployment. Defaults to https://clawrouter.openclaw.ai.

## The report: the router's requests, tokens, and cost this month, its
## budget, and each routed provider. Provider names and the budget
## ledger's name stay as ClawRouter sends them.

# The account line: how many providers the router sent requests to.
usage-clawrouter-routed-count = { $count } routed providers
# The plan line, with a monthly budget set or without one.
usage-clawrouter-managed-budget = Managed monthly budget
usage-clawrouter-unmetered = Unmetered
usage-clawrouter-requests = Requests
# All $requests, then how many $succeeded and $failed; each a grouped
# number, such as 1,200.
usage-clawrouter-request-counts = { $requests } · { $succeeded } succeeded · { $failed } failed
usage-clawrouter-tokens = Tokens
# All $tokens, then how many were $input and $output; each a grouped
# number.
usage-clawrouter-token-counts = { $tokens } · { $input } input · { $output } output
usage-clawrouter-actual-cost = Actual cost
usage-clawrouter-budget-ledger = Budget ledger
usage-clawrouter-monthly-budget = Monthly budget
# $amount is spent of the budget, such as $0.012000 / $25.00; $remaining
# the dollars left.
usage-clawrouter-budget-remaining = { $amount } · { $remaining } remaining
# The balance: dollars spent this month.
usage-clawrouter-spent-this-month = Spent this month
usage-clawrouter-routed-providers = Routed providers
# A provider's month: $requests and $tokens are grouped numbers, $cost
# dollars such as $0.004000.
usage-clawrouter-provider-usage = { $requests } requests · { $cost } · { $tokens } tokens
usage-clawrouter-provider-cost = Provider cost
# A provider ClawRouter gives no name.
usage-clawrouter-unknown = Unknown
