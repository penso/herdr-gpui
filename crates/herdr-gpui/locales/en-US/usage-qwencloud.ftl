# Qwen Cloud's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-qwencloud-setting-cookie = Your Qwen Cloud console session. Sign in at https://home.qwencloud.com/billing/subscription/token-plan-individual, open Developer Tools > Application > Cookies > https://home.qwencloud.com, and copy the login ticket (login_qwencloud_ticket or login_aliyunid_ticket) together with login_aliyunid_csrf and cna. Paste them as "name=value; name2=value2", or copy the whole Cookie header of the data/api.json request from the Network tab.
usage-qwencloud-setting-sec-token = Optional. The console's sec_token, when it cannot be read from /tool/user/info.json: in Developer Tools > Network, the sec_token form field of the data/api.json request on the Token Plan page.
