# Settings > Machine sync: keeping the saved SSH devices of the user's
# computers in step, so each one reaches all the others.

settings-machine-sync-title = Machine sync
settings-machine-sync-unavailable = Machine sync runs only in the app you launch yourself.
settings-machine-sync-off = Off. Turning it on lets each of your computers reach all the others over SSH, from whichever one you use Herdr UI on.
settings-machine-sync-waiting = On. The first sync starts shortly.
settings-machine-sync-running = Syncing…
# $ago is how long ago it finished, such as "5 minutes ago".
settings-machine-sync-done = Synced { $ago } · reached { $reached } of { $machines ->
        [one] { $machines } machine
       *[other] { $machines } machines
    }
settings-machine-sync-failed = The last sync failed: { $error }
settings-machine-sync-failure = Could not read or save machine sync: { $error }
settings-machine-sync-now = Sync now

settings-machine-sync-confirm = Turn on machine sync?
# $names lists the saved devices, separated by commas.
settings-machine-sync-confirm-machines = This computer and { $names } will each:
settings-machine-sync-confirm-none = This computer has no saved SSH devices yet. Machine sync starts with the first one you add.
settings-machine-sync-confirm-changes = Create an SSH key of their own, ~/.ssh/herdr_ed25519; accept every other machine's herdr key in ~/.ssh/authorized_keys, where each line ends in "herdr-sync"; save the machines they lack as devices; and keep the list in step from then on, also when Herdr UI runs there. Nothing they already have is changed or removed.
settings-machine-sync-confirm-ssh = The others reach this computer by the address below, so it needs SSH turned on (Remote Login on macOS). Herdr UI offers the herdr key itself; your SSH config is never edited.
settings-machine-sync-confirm-on = Turn on
settings-machine-sync-confirm-cancel = Cancel

settings-machine-sync-section-this = This computer
settings-machine-sync-address = Address
settings-machine-sync-address-note = How your other computers reach this one, as user@host: a Tailscale or local network name works best.
settings-machine-sync-name = Name
# An example name for this computer, shown in the empty field.
settings-machine-sync-name-placeholder = Laptop
settings-machine-sync-name-note = What your other computers call this one.
settings-machine-sync-save = Save

settings-machine-sync-section-machines = Machines
settings-machine-sync-machines-tick = Tick each machine to keep in sync with this one. Untick one to leave it out.
settings-machine-sync-machines-none = No saved SSH devices yet. Add one from the device menu.
settings-machine-sync-machines-loading = Reading the saved devices…
settings-machine-sync-machine-disabled = Turned off in Herdr
settings-machine-sync-machine-left-out = Left out of sync
settings-machine-sync-machine-pending = Not synced yet
settings-machine-sync-machine-done = Synced · { $keys ->
        [one] { $keys } key
       *[other] { $keys } keys
    } and { $devices ->
        [one] { $devices } device
       *[other] { $devices } devices
    } added
settings-machine-sync-machine-failed = Not reached: { $error }
# $names lists the devices, separated by commas.
settings-machine-sync-learned = Saved here from your other machines: { $names }

settings-machine-sync-description = Keep the saved SSH devices of all your computers in step.
settings-machine-sync-machines-description = Which saved devices take part in machine sync.
