# Hugging Face's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-huggingface-setting-api-key = A Hugging Face access token from https://huggingface.co/settings/tokens. A classic read token works; a fine-grained token needs the Billing read permission. Not needed when `hf auth login` has saved a token on the host.
usage-huggingface-setting-cookie = Optional, for the prepaid credit balance. Sign in at https://huggingface.co/settings/billing with the token's account, open Developer Tools > Application > Cookies > https://huggingface.co, and copy the token cookie as "token=value" (or every cookie there as "name=value; name2=value2").
