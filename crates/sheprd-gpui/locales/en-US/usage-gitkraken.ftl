# GitKraken AI's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-gitkraken-setting-token = A GitKraken account session access token. Sign in at https://gitkraken.dev/account#ai-usage, open Developer Tools > Network, select the successful ai-tasks/usage request, and copy only the value after "Bearer " in its Authorization header. Replace it when it expires.
usage-gitkraken-setting-org-id = Optional organization ID: the gk-org-id header of the same request, to read that organization's shared pool.

## The report: weekly personal credits and an organization's shared pool.
## $amount, $used, and $limit are credit counts as written, such as 1,200 or
## 55.50; $count is the number a plural form is picked by.

usage-gitkraken-weekly-usage = Weekly usage
usage-gitkraken-personal = Personal
# The organization's credits, which its members share.
usage-gitkraken-shared-pool = Shared pool
usage-gitkraken-your-share = Your shared usage
# The shared pool's credits other members used.
usage-gitkraken-rest = Rest of organization
usage-gitkraken-credits = { $count ->
       *[other] { $amount } credits
    }
# Credits used of a weekly allowance; $count is the allowance.
usage-gitkraken-used-of = { $count ->
       *[other] { $used } / { $limit } credits used
    }
# Credits used without a limit; $count is the credits used.
usage-gitkraken-used-unlimited = { $count ->
       *[other] { $used } credits used · Unlimited
    }
# Credits used with no allowance at all; $count as above.
usage-gitkraken-used-no-allowance = { $count ->
       *[other] { $used } credits used · No allowance
    }
