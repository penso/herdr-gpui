# Helmcode's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-helmcode-setting-cookie = The dashboard session. Sign in to https://cloud.helmcode.com/dashboard (or https://cloud.nan.builders/dashboard for NaN Builders), open Developer Tools > Network, reload, and copy the Cookie request header of a request to cloud-api.helmcode.com (or cloud-api.nan.builders). Developer Tools > Application > Cookies lists the same cookies. Paste it as "name=value; name2=value2".
usage-helmcode-setting-tenant = Which dashboard a pasted cookie belongs to: "helmcode" (default) or "nanBuilders".

## The report: each model's token quota and the prepaid balance. Model and
## tenant names, such as Helmcode Cloud, stay as Helmcode names them.

usage-helmcode-model-quotas = Model quotas
usage-helmcode-prepaid-balance = Prepaid balance
# A model's tokens used of its cap: $used and $cap are token counts as
# written, such as 250,000 and 1,000,000; $count is the cap, for the plural.
usage-helmcode-tokens = { $count ->
       *[other] { $used } / { $cap } tokens
    }
# The same, with the tokens paid from prepaid credit ($credit, such as 5,000).
usage-helmcode-tokens-credit = { $count ->
       *[other] { $used } / { $cap } tokens · { $credit } credit-funded
    }
