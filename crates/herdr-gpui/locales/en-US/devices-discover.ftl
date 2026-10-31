# The Nearby devices list under Add Device's SSH target field.

devices-discover-heading = Nearby devices
devices-discover-waiting = Waiting for macOS to allow Herdr on your local network…
# $sources lists where the search looks, such as "SSH config, Tailscale, and
# Bonjour".
devices-discover-searching = Searching { $sources }…
devices-discover-found = { $count ->
        [0] Search finished. No devices found.
        [one] Search finished. Found { $count } device.
       *[other] Search finished. Found { $count } devices.
    }
devices-discover-again = Search again
# Where a host was found: the user's ~/.ssh/config. Tailscale and Bonjour
# keep their names.
devices-discover-ssh-config = SSH config
# Why a source found nothing. $source is one of the names above.
devices-discover-failed = { $source }: { $error }
# Lists of sources, built the way CLDR builds lists: two names, then a longer
# list's leading and closing parts.
devices-discover-list-two = { $first } and { $second }
devices-discover-list-middle = { $start }, { $next }
devices-discover-list-end = { $start }, and { $last }
