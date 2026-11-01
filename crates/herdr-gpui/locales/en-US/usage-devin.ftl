# Devin's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-devin-setting-token = Sign in to https://app.devin.ai, open your organization's Usage & Limits page, then in Developer Tools > Network select the successful billing/quota/usage request and copy its Authorization request header without the leading "Bearer ". The value is a browser session token and expires with that session.
usage-devin-setting-organization = The x-cog-org-id request header of the same request (an org-… or org_… id). An organization slug or its app.devin.ai URL also works.
