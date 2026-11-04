# Codebuff's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-codebuff-setting-api-key = A Codebuff API key from https://www.codebuff.com. It reads the credit balance only; without it, the session from `codebuff login` is used, which also reads the plan and weekly limit.
usage-codebuff-setting-base-url = An https URL that replaces https://www.codebuff.com, e.g. for a staging server. Other schemes are ignored so the token never travels unencrypted.

## The report: the account's credits, weekly limit, and billing settings.
## Plan names and the subscription's status (such as Active) stay as
## Codebuff sends them.

# The limit: how much of the credit quota is spent.
usage-codebuff-credits = Credits
# The balance: credits left.
usage-codebuff-remaining = Remaining
usage-codebuff-account = Account
usage-codebuff-subscription = Subscription
# Whether credits are bought automatically when they run low.
usage-codebuff-auto-top-up = Auto top-up
usage-codebuff-on = On
usage-codebuff-off = Off
