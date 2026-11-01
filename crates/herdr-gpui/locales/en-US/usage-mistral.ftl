# Mistral's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-mistral-setting-cookie = Sign in to https://admin.mistral.ai/organization/usage, open Developer Tools > Application > Cookies for https://admin.mistral.ai, and copy the ory_session_… cookie (its name ends in a per-project suffix) and csrftoken. Paste them as "name=value; name2=value2"; the Cookie header of any admin.mistral.ai request also works.

## The report: this month's API spend and tokens, the subscription's
## included API and Vibe allowances, and credits. Vibe is the product's
## name, and model names stay as Mistral sends them.

usage-mistral-api-spend = API spend this month
usage-mistral-credits = Credits
# A window and a fact: the API spend the subscription includes.
usage-mistral-included-api = Included API
# An allowance's spend: $used and $limit are amounts, such as 0.51 EUR and
# 25.50 EUR.
usage-mistral-allowance-used = { $used } of { $limit } used
usage-mistral-allowances = Allowances
usage-mistral-tokens-this-month = Tokens this month
usage-mistral-input = Input
usage-mistral-output = Output
usage-mistral-cached = Cached
# How many models used tokens this month.
usage-mistral-models = Models
usage-mistral-spend-by-model = Spend by model
