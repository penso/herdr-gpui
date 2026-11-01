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
