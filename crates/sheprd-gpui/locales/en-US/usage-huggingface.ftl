# Hugging Face's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-huggingface-setting-api-key = A Hugging Face access token from https://huggingface.co/settings/tokens. A classic read token works; a fine-grained token needs the Billing read permission. Not needed when `hf auth login` has saved a token on the host.
usage-huggingface-setting-cookie = Optional, for the prepaid credit balance. Sign in at https://huggingface.co/settings/billing with the token's account, open Developer Tools > Application > Cookies > https://huggingface.co, and copy the token cookie as "token=value" (or every cookie there as "name=value; name2=value2").

## The report: month-to-date Inference Providers charges, the ZeroGPU
## quota, prepaid credits, and the account. User names and the PRO plan's
## name stay as Hugging Face sends them.

# Hugging Face's product for running models through partner providers.
usage-huggingface-inference-providers = Inference Providers
usage-huggingface-billable = Billable usage
usage-huggingface-gross = Gross inference usage
usage-huggingface-included = Included inference amount
usage-huggingface-spending-limit = Spending limit
usage-huggingface-requests = Requests
# Hugging Face's free GPU time for Spaces.
usage-huggingface-zerogpu = ZeroGPU
usage-huggingface-gpu-used = GPU time used
usage-huggingface-gpu-left = GPU time remaining
# $minutes is a number of minutes, such as 10 or 2.5.
usage-huggingface-minutes = { $minutes } min
usage-huggingface-month-spend = Inference this month
usage-huggingface-prepaid = Prepaid credits
usage-huggingface-account = Account
usage-huggingface-user = User
# The plan line of an account without PRO.
usage-huggingface-free = Free
