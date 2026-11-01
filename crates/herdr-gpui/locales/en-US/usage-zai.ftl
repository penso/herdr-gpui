# z.ai / GLM's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-zai-setting-api-key = A z.ai API key from https://z.ai/manage-apikey/apikey-list, or for BigModel China one from https://bigmodel.cn/usercenter/proj-mgmt/apikeys.
usage-zai-setting-region = "global" for api.z.ai (the default) or "bigmodel-cn" for open.bigmodel.cn, the China mainland GLM Coding Plan.
usage-zai-setting-api-host = An HTTPS host that replaces the region's API host, e.g. a proxy.
usage-zai-setting-quota-url = A full HTTPS URL that replaces the quota endpoint.
usage-zai-setting-balance-url = A full HTTPS URL that replaces the BigModel China balance endpoint.
usage-zai-setting-usage-scope = "personal" (the default) or "team" for BigModel team usage, which also needs organization and project.
usage-zai-setting-organization = For team usage: open https://bigmodel.cn/coding-plan/team/usage-stats, pick the team, and in Developer Tools > Network copy the Bigmodel-Organization request header of the quota/limit request.
usage-zai-setting-project = For team usage: the Bigmodel-Project request header of the same request.

## The report: the coding plan's token or credit quotas, the MCP allowance,
## and tokens per model over the last 30 days. Model names and plan names
## stay as Z.ai sends them, and MCP is the protocol's name.

# A quota window Z.ai sends in hours; $hours is how many, such as 3.
usage-zai-hours = { $hours }-hour
# A quota window of a length Z.ai does not say.
usage-zai-quota = Quota
usage-zai-quota-details = Quota details
usage-zai-credit-quota = Credit quota
usage-zai-token-quota = Token quota
usage-zai-session-credit-quota = Session credit quota
usage-zai-session-token-quota = Session token quota
usage-zai-mcp-quota = MCP quota
# How much of a quota is used; $percent is a number such as 22.4, $limit
# and $remaining counts such as 1000 and 776, and $counts says which of
# those two Z.ai sent.
usage-zai-detail = { $percent }% used{ $counts ->
        [both] { " · " }{ $limit } limit · { $remaining } remaining
        [limit] { " · " }{ $limit } limit
        [remaining] { " · " }{ $remaining } remaining
       *[none] {""}
    }
# Credit plans cost double at peak hours, weekdays 06:00 to 10:00 UTC.
usage-zai-quota-rate = Quota rate
usage-zai-peak = Peak
usage-zai-off-peak = Off-peak
usage-zai-account-balance = Account balance
usage-zai-tokens-last-30-days = Tokens, last 30 days
# A model Z.ai reports tokens for without its name.
usage-zai-unknown-model = Unknown
