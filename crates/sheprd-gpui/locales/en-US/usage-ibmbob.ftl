# IBM Bob's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-ibmbob-setting-api-key = An IBM Bob API key that can read subscription usage, created at https://bob.ibm.com (the same key Bob Shell uses as BOBSHELL_API_KEY).
usage-ibmbob-setting-token = Instead of an API key, an IBM Cloud bearer token (a JWT) for your Bob account. It is sent as "Authorization: Bearer <token>".

## The report: Bobcoins used per team against each team's budget.
## Bobcoins are Bob's credits; instance, team, and plan names stay as Bob
## sends them.

usage-ibmbob-bobcoins-used = Bobcoins used
usage-ibmbob-bobcoin-usage = Bobcoin usage
# A team's Bobcoins used of its budget: $used and $limit are amounts as
# written, such as 10 and 40; $count is the budget, for the plural.
usage-ibmbob-used-of = { $count ->
       *[other] { $used } / { $limit } Bobcoins
    }
# A team's Bobcoins used without a budget; $count is the amount used.
usage-ibmbob-used = { $count ->
       *[other] { $used } Bobcoins used
    }
