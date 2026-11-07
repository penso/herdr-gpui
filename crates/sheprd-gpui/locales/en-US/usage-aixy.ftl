# Aixy's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-aixy-setting-api-key = The project-scoped Aixy API key your workload uses, from https://dash.aixy-gateway.com under the project's API keys. Usage is that key's own traffic from every machine.
usage-aixy-setting-base-url = Optional gateway URL for a self-hosted or dedicated Aixy, with or without /v1. Defaults to https://api.aixy-gateway.com. It must be HTTPS unless it is on localhost, a private network, or a .local host.

## The report: the budgets that apply to the key, and the key's own usage
## over the last seven days. Key and project names stay as Aixy sends them.

usage-aixy-key-title = Aixy key
usage-aixy-key = Key
usage-aixy-project = Project
# A project Aixy names neither by name nor by ID.
usage-aixy-unknown = Unknown
# When Aixy read the figures; the value is its timestamp.
usage-aixy-observed = Observed
usage-aixy-budgets = Applicable budgets
# A budget's name: who it covers ($scope), how often it resets ($interval),
# whether it is shared or personal ($sharing), and whether it stops
# requests or only watches them ($enforcement).
usage-aixy-budget = { $scope ->
        [organization] Organization
        [project] Project
        [team] Team
        [user] User
       *[api_key] Key
    } · { $interval ->
        [daily] Daily
        [weekly] Weekly
        [monthly] Monthly
       *[lifetime] Lifetime
    } · { $sharing ->
        [shared] Shared
       *[personal] Personal
    } · { $enforcement ->
        [hard] Hard
       *[monitor] Monitor
    }
# A budget's short name as a limit, such as "Project monthly"; $scope and
# $interval as in usage-aixy-budget.
usage-aixy-budget-short = { $scope ->
        [organization] Organization
        [project] Project
        [team] Team
        [user] User
       *[api_key] Key
    } { $interval ->
        [daily] daily
        [weekly] weekly
        [monthly] monthly
       *[lifetime] lifetime
    }
# A budget's state, in dollars such as $70.00: $left of $limit left, and
# $spent so far.
usage-aixy-budget-left = { $left } of { $limit } left, { $spent } spent
# The same for a budget that stops requests, which also holds $reserved for
# requests still running.
usage-aixy-budget-reserved = { $left } of { $limit } left, { $spent } spent, { $reserved } reserved
usage-aixy-unavailable = Unavailable
# The balance: what the key's last seven days cost, as far as Aixy could
# attribute it.
usage-aixy-attributed-week = Last 7 days, attributed
usage-aixy-week = Last 7 days · this key
usage-aixy-requests = Requests
usage-aixy-tokens = Tokens
usage-aixy-attributed-spend = Attributed spend
usage-aixy-cost-coverage = Cost coverage
# How many requests have a cost: $attributed of all $requests, of which
# $partial only in part. Each is a grouped number, such as 1,200.
usage-aixy-coverage = { $attributed } / { $requests } requests, { $partial } partial
