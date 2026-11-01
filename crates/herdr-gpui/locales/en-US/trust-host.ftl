# Reviewing the host key SSH refused for a saved device, opened from the
# reconnecting card. $host is the device's name; $target is its SSH target.

trust-host-title = Trust the host key of { $host }?
trust-host-fetching = Fetching the host key of { $target }…
trust-host-trust = Trust
trust-host-trusting = Trusting…
trust-host-reject = Don't Trust
trust-host-close = Close
# $error is why the key could not be fetched or saved.
trust-host-failed = Could not review the host key of { $target }: { $error }
trust-host-trusted = Trusted the host key of { $host }; reconnecting
# A key that differs from the one trusted before, shown beside it.
trust-host-changed = The host key of { $target } differs from the one you trusted before. That is expected after the host was reinstalled or its keys were regenerated; otherwise someone could be intercepting the connection.
trust-host-before = Trusted before:
trust-host-now = Offered now:
trust-host-replace-note = Replacing removes the old key from { $known_hosts }, keeping a copy beside it ending in .old, and adds the new one.
trust-host-replace = Replace Key
