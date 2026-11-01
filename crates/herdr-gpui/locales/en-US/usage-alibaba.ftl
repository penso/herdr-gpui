# Alibaba's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-alibaba-setting-cookie = Your Model Studio console session. Sign in to the Coding Plan page at https://modelstudio.console.alibabacloud.com (or https://bailian.console.aliyun.com in China), open Developer Tools > Application > Cookies for that site, and copy at least login_aliyunid_ticket, login_aliyunid_pk, login_aliyunid_csrf and cna. Paste them as "name=value; name2=value2", or copy the whole Cookie header of the data/api.json request from the Network tab.
usage-alibaba-setting-api-key = A Coding Plan API key (sk-sp-...) from the Coding Plan page of the Model Studio console. Used when no console cookie works; some mainland accounts only answer the cookie.
usage-alibaba-setting-region = Optional. "intl" (modelstudio.console.alibabacloud.com) or "cn" (bailian.console.aliyun.com). Unset tries International, then China mainland.
usage-alibaba-setting-sec-token = Optional. The console's sec_token, when it cannot be read from /tool/user/info.json: in Developer Tools > Network, the sec_token form field of the data/api.json request on the Coding Plan page.
