# Alibaba Token Plan's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-alibabatokenplan-setting-region = Which Token Plan to read: "intl" (International Team, the default), "cn" (China mainland Team), "intl-personal" or "cn-personal" (Personal/Solo).
usage-alibabatokenplan-setting-cookie = Only needed without a signed-in Bailian CLI (bl). Sign in to the Token Plan page (https://modelstudio.console.alibabacloud.com, or https://bailian.console.aliyun.com in China), open Developer Tools > Application > Cookies for that site, and copy at least login_aliyunid_ticket, login_aliyunid_pk, login_aliyunid_csrf and cna. Paste them as "name=value; name2=value2", or copy the whole Cookie header of the data/api.json request from the Network tab.
usage-alibabatokenplan-setting-sec-token = Optional. The console's sec_token, when it cannot be read from /tool/user/info.json: in Developer Tools > Network, the sec_token form field of any data/api.json request on the Token Plan page.
