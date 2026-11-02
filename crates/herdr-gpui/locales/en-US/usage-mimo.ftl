# Xiaomi MiMo's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-mimo-setting-cookie = The MiMo console session. Sign in at https://platform.xiaomimimo.com/#/console/balance, open Developer Tools → Application → Cookies → https://platform.xiaomimimo.com, copy the api-platform_serviceToken and userId cookies (and api-platform_ph and api-platform_slh when present), and paste them as "api-platform_serviceToken=value; userId=value".
usage-mimo-setting-base-url = Optional HTTPS API URL. Defaults to https://platform.xiaomimimo.com/api/v1.

## The report: the balance, what of it was paid or granted, and the token
## plan's monthly credits. The plan's name stays as MiMo sends it.

usage-mimo-paid = Paid
usage-mimo-granted = Granted
usage-mimo-token-plan = Token plan
# $used and $limit are credit counts, such as 10,100,158 and 200,000,000;
# $count is the limit as a number, for the plural.
usage-mimo-credits-used = { $count ->
        [one] { $used } of { $limit } credit
       *[other] { $used } of { $limit } credits
    }
# The token plan's state once its period has ended.
usage-mimo-expired = Expired
usage-mimo-balance = Balance
# A section's title: the account's balance and token plan.
usage-mimo-account = Account
