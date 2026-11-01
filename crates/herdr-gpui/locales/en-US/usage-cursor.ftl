# Cursor's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-cursor-setting-cookie = Your cursor.com session. Sign in at https://cursor.com/dashboard, open Developer Tools > Application > Cookies > https://cursor.com, and copy the WorkosCursorSessionToken cookie (or __Secure-next-auth.session-token on older accounts). Paste it as "WorkosCursorSessionToken=value". Not needed when Cursor is listed in show_providers and you are signed in with Chrome or Safari.

## The report: the billing cycle's included, Auto, and API usage, the
## on-demand budget, and the legacy request quota. Plan names, such as
## Cursor Pro, and the Grok Bot allowance's name stay as Cursor names them.

# The billing cycle's limit on Auto and Composer requests.
usage-cursor-auto = Auto + Composer
# The billing cycle's limit on requests to named API models.
usage-cursor-api = API
usage-cursor-request-quota = Request quota
usage-cursor-included = Included usage
usage-cursor-this-cycle = This cycle
# $used and $limit are dollar amounts, such as $15.00 and $50.00.
usage-cursor-of = { $used } of { $limit }
# The on-demand budget: $budget is team for a budget the whole team shares,
# or personal for the member's own.
usage-cursor-budget = { $budget ->
        [team] Team on-demand
       *[personal] On-demand
    }
# The balance of an on-demand budget with a cap; $budget as above.
usage-cursor-budget-left = { $budget ->
        [team] Team on-demand left
       *[personal] On-demand left
    }
# The spend of an on-demand budget without a cap; $budget as above.
usage-cursor-budget-spend = { $budget ->
        [team] Team on-demand spend
       *[personal] On-demand spend
    }
usage-cursor-spent = Spent this cycle
# The member's own part of a team's on-demand spend.
usage-cursor-your-share = Your share
