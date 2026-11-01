# Mistral's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-mistral-setting-cookie = Sign in to https://admin.mistral.ai/organization/usage, open Developer Tools > Application > Cookies for https://admin.mistral.ai, and copy the ory_session_… cookie (its name ends in a per-project suffix) and csrftoken. Paste them as "name=value; name2=value2"; the Cookie header of any admin.mistral.ai request also works.
