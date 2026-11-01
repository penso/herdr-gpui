# Codebuff's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-codebuff-setting-api-key = A Codebuff API key from https://www.codebuff.com. It reads the credit balance only; without it, the session from `codebuff login` is used, which also reads the plan and weekly limit.
usage-codebuff-setting-base-url = An https URL that replaces https://www.codebuff.com, e.g. for a staging server. Other schemes are ignored so the token never travels unencrypted.
