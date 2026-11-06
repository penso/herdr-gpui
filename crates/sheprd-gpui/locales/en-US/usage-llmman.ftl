# llmman's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-llmman-setting-base-url = The llmman serve address, when not http://127.0.0.1:17434. A host without a scheme uses HTTP on port 17434, e.g. localhost:18000. Plain HTTP is allowed only for loopback, private-network, and .local hosts; others need HTTPS. Without it, LLMMAN_HOST on the probed host is used when set.
usage-llmman-setting-api-key = Only when the daemon was started with LLMMAN_API_KEYS: one of those keys.

## The report: the daemon's model memory, its loaded and stored models,
## and its version. Model names, counts, and sizes (such as 10.0 GB) stay
## as figures.

# A window and a fact: the model memory loaded weights fill.
usage-llmman-memory = Memory
# $used and $total are sizes, such as 10.0 GB and 40.0 GB.
usage-llmman-memory-used = { $used } of { $total }
usage-llmman-loaded = Loaded
usage-llmman-stored = Stored
usage-llmman-version = Version
# A section's title: the daemon's summary.
usage-llmman-daemon = Daemon
usage-llmman-loaded-models = Loaded models
# The plan line: read with a key, or from a daemon open to this host.
usage-llmman-api-key = API key
usage-llmman-local-daemon = Local daemon
