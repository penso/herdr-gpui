# v0's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-v0-setting-api-key = A v0 Platform API key from https://v0.app/settings/keys.
usage-v0-setting-scope = Optional project ID or slug to read that scope's billing and rate limit.

## The report: the billing allowance, the on-demand balance, and the API
## rate limit. The billing type (token or legacy) and the project scope stay
## as v0 sends them.

usage-v0-api = v0 API
usage-v0-rate-limit = Rate limit
usage-v0-billing-remaining = Billing remaining
usage-v0-on-demand-balance = On-demand balance
usage-v0-rate-limit-remaining = Rate-limit remaining
usage-v0-billing-type = Billing type
usage-v0-scope = Scope
# What is left of a limit; $remaining and $limit are grouped numbers, such
# as 750 and 1,000.
usage-v0-remaining-of = { $remaining } of { $limit }
# A limit whose remaining amount v0 does not report; $limit as above.
usage-v0-unavailable = Unavailable (limit { $limit })
