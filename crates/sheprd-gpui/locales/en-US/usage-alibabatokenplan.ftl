# Alibaba Token Plan's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-alibabatokenplan-setting-region = Which Token Plan to read: "intl" (International Team, the default), "cn" (China mainland Team), "intl-personal" or "cn-personal" (Personal/Solo).
usage-alibabatokenplan-setting-cookie = Only needed without a signed-in Bailian CLI (bl). Sign in to the Token Plan page (https://modelstudio.console.alibabacloud.com, or https://bailian.console.aliyun.com in China), open Developer Tools > Application > Cookies for that site, and copy at least login_aliyunid_ticket, login_aliyunid_pk, login_aliyunid_csrf and cna. Paste them as "name=value; name2=value2", or copy the whole Cookie header of the data/api.json request from the Network tab.
usage-alibabatokenplan-setting-sec-token = Optional. The console's sec_token, when it cannot be read from /tool/user/info.json: in Developer Tools > Network, the sec_token form field of any data/api.json request on the Token Plan page.

## The report: the token plan's credits, for a team subscription or a
## personal plan's windows. Qwen Cloud's report uses these words too. Plan
## names stay as Alibaba sends them.

# The balance: credits left of the subscription's total.
usage-alibabatokenplan-credits-left = Credits left
# Shown for a team account without an active token plan.
usage-alibabatokenplan-subscription = Subscription
usage-alibabatokenplan-active-plans = Active token plans
usage-alibabatokenplan-none = None
usage-alibabatokenplan-credits-used = Credits used
# A window's name, beside its credits used.
usage-alibabatokenplan-window = { $kind ->
        [session] Session
        [weekly] Weekly
       *[monthly] Monthly
    }
# Credits used in a window: $used of $total, each a grouped number such as
# 5,000.
usage-alibabatokenplan-credits-of = { $used } / { $total } credits
