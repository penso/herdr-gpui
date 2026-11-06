# LiteLLM's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-litellm-setting-api-key = A LiteLLM virtual key (sk-…) issued by your LiteLLM proxy, e.g. from its admin UI under Virtual Keys. A master key is not needed.
usage-litellm-setting-base-url = The LiteLLM proxy's URL, with or without /v1, e.g. https://litellm.example.com. It must be HTTPS unless it is on localhost, a private network, or a .local host.

## The report: the key's personal and team budgets, or its spend this month
## when the proxy hides the budgets. Team aliases ($team, such as ai) stay
## as the proxy names them.

# A budget's window and balance, by whose it is.
usage-litellm-personal-budget = Personal budget
usage-litellm-team-budget = Team budget
usage-litellm-team-named-budget = Team { $team } budget
# The same, for spend without a budget limit.
usage-litellm-personal-spend = Personal spend
usage-litellm-team-spend = Team spend
usage-litellm-team-named-spend = Team { $team } spend
# Spend summed from a self-scoped spend report, with no budget known.
usage-litellm-key-spend-only = Key spend only
usage-litellm-user-spend-only = User spend only
# A section's title: the virtual key, its team, and when it expires.
usage-litellm-key = Key
usage-litellm-team = Team
usage-litellm-key-expires = Key expires
# The plan line: the report is read with a virtual key.
usage-litellm-api-key = API key
usage-litellm-spend-report = Spend report
usage-litellm-period = Period
# $start and $end are the report's first and last days, such as 2026-09-01.
usage-litellm-period-range = { $start } – { $end } UTC
