# Bifrost's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-bifrost-setting-api-key = Your Bifrost virtual key (vk-…), from the gateway's Virtual Keys page. It is sent as the x-bf-vk header to base_url only.
usage-bifrost-setting-base-url = Your Bifrost gateway's URL, e.g. https://bifrost.example.com. Bifrost has no public host. It must be HTTPS unless it is on localhost, a private network, or a .local host.

## The report: the virtual key's budgets, rate limits, and model spend.
## The key's name, budget and pool names, provider and model names, and
## budget IDs stay as Bifrost sends them.

# A limit set for one provider; $name is the provider's name, or its
# number in the key's list.
usage-bifrost-provider-scope = Provider { $name }
# A limit set for one model; $name is the model's name, after its
# provider's when Bifrost gives one (openai · gpt-4o), or its number.
usage-bifrost-model-scope = Model { $name }
# A rate limit's dimension.
usage-bifrost-tokens = Tokens
usage-bifrost-requests = Requests
# A rate limit drawn from a named pool: $source is the pool's name, such
# as Team pool, and $limit what it limits (usage-bifrost-tokens or
# usage-bifrost-requests).
usage-bifrost-sourced-limit = { $source } { $limit }
# A rate limit for one provider or model: $scope is a scope above, and
# $limit the limit's name.
usage-bifrost-scoped-limit = { $scope } { $limit }
# A budget without a name of its own.
usage-bifrost-budget = Budget
# A budget for one provider or model: $scope is a scope above, and $budget
# the budget's name.
usage-bifrost-scoped-budget = { $scope } · { $budget }
# A budget's calendar period, as a limit's name after the budget's
# ($budget), in a balance's name, and after an amount ($amount, such as
# $42.17 / $125.00).
usage-bifrost-periodic = { $budget } · { $period ->
        [hourly] Hourly
        [daily] Daily
        [weekly] Weekly
        [monthly] Monthly
        [quarterly] Quarterly
       *[yearly] Yearly
    }
usage-bifrost-period = { $period ->
        [hourly] Hourly
        [daily] Daily
        [weekly] Weekly
        [monthly] Monthly
        [quarterly] Quarterly
       *[yearly] Yearly
    }
usage-bifrost-amount-period = { $amount } · { $period ->
        [hourly] Hourly
        [daily] Daily
        [weekly] Weekly
        [monthly] Monthly
        [quarterly] Quarterly
       *[yearly] Yearly
    }
# The balance of a budget without a limit: only what it spent.
usage-bifrost-spend = Spend
# A budget without a pool name, by its ID ($id); $budget is its name.
usage-bifrost-budget-id = { $budget } { $id }
usage-bifrost-rate-limits = Rate limits
# A rate limit whose maximum Bifrost does not report.
usage-bifrost-unavailable = Unavailable
# Shown while Bifrost has turned the virtual key off.
usage-bifrost-virtual-key = Virtual key
usage-bifrost-status = Status
usage-bifrost-inactive = Inactive
usage-bifrost-budgets = Budgets
usage-bifrost-models = Models
# A model row without a model name.
usage-bifrost-model = Model
# A model's spend: $cost in dollars, such as $5.00, and $tokens an
# abbreviated count, such as 1.2M.
usage-bifrost-model-cost = { $cost } · { $tokens } tokens
# How many more models spent, beyond those listed.
usage-bifrost-other-models = Other models
