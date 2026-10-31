# A saved device's menu, from its sidebar host header, and its pages:
# rename, remove, forward a port, and the terminal theme.

host-menu-rename = Rename…
# A switch: the device uses the keys its Herdr server binds.
host-menu-server-keybindings = Use server keybindings
host-menu-forward-port = Forward port…
host-menu-theme = Terminal theme…
host-menu-remove = Remove device…
host-menu-cloud-settings = Cloud device settings…
host-menu-run-setup = Run setup script
host-menu-local-keybindings = Using local keybindings: { $error }
host-menu-remove-failed = Remove { $device }: { $error }
host-menu-github-left = Removed { $device }, but could not delete its GitHub sign-in: { $error }

# Removing a device. $device is its name, $target its SSH target or machine,
# $session its Herdr session.
host-menu-remove-title = Remove device
host-menu-remove-ssh-note = Remove { $device } ({ $target } · { $session }) from this computer? Herdr keeps running on that host.
# $provider is a cloud provider, such as Azure; $machine is what it calls a
# machine, such as VM or workspace.
host-menu-remove-cloud-note = Remove { $device } ({ $target }) from this computer? The { $provider } { $machine } itself is left as it is.
host-menu-remove-github = Also delete its GitHub sign-in from this computer
host-menu-cancel = Cancel
host-menu-remove-submit = Remove

host-menu-rename-title = Rename device
host-menu-close = Close
host-menu-rename-name = Name
host-menu-rename-submit = Rename
host-menu-renaming = Renaming…

# Deleting a cloud device at its provider. $machine is what the provider
# calls a machine, such as VM or workspace.
host-menu-delete-checking = Checking…
host-menu-delete-anyway = Delete anyway
host-menu-delete = Delete { $machine }
host-menu-unsynced-checking = Checking the { $machine } for work not pushed…
host-menu-unsynced-failed = Could not check the { $machine } for work not pushed. Anything on it is deleted with it.
host-menu-unsynced-found = Work on this { $machine } is pushed nowhere else:
# One checkout with work pushed nowhere: its path, then what it holds.
host-menu-unsynced-changed = { $path } ({ $changed ->
        [one] { $changed } changed file
       *[other] { $changed } changed files
    })
host-menu-unsynced-unpushed = { $path } ({ $unpushed ->
        [one] { $unpushed } unpushed commit
       *[other] { $unpushed } unpushed commits
    })
host-menu-unsynced-both = { $path } ({ $changed ->
        [one] { $changed } changed file
       *[other] { $changed } changed files
    }, { $unpushed ->
        [one] { $unpushed } unpushed commit
       *[other] { $unpushed } unpushed commits
    })
host-menu-unsynced-more = and { $count } more
host-menu-unsynced-advice = Push it, or Teleport it to another device, before deleting.
# $device is the device's name, $target its machine, $provider the cloud
# provider, and $leftovers what else the provider deletes with it, as the
# provider words it.
host-menu-delete-note = Delete the { $machine } { $device } ({ $target }) at { $provider }{ $leftovers } and remove it from this computer? Its files are not kept, and this cannot be undone.
host-menu-remove-only = Remove from Herdr only

# The device's forwarded ports, under its menu's rows, and the dialog that
# forwards one.
host-menu-forwards = Forwarded ports
host-menu-forward-connecting = connecting…
host-menu-forward-ended = ended
host-menu-forward-open = Open
host-menu-forward-dismiss = Dismiss
host-menu-forward-stop = Stop
host-menu-forward-title = Forward port
host-menu-forward-close = Close
host-menu-forward-intro = Reach a port that a program on { $device } listens on from this computer, over SSH. It stays forwarded until you stop it.
host-menu-forward-remote-port = Remote port
host-menu-forward-submit = Forward

# The device's terminal theme picker. $device is the device's name.
host-menu-theme-title = Terminal theme · { $device }
host-menu-theme-close = Close
host-menu-theme-search = Search themes...
# The row that drops the device's own theme for the app's.
host-menu-theme-same-as-app = Same as app
# Marks the theme the device uses now.
host-menu-theme-current = Current
host-menu-theme-hover-hint = Only this device's terminal changes. Hover to preview; Enter or click to save.
host-menu-theme-select-hint = Only this device's terminal changes; select the device to preview it.
