# The Add Device dialog: its frame and rail, and the SSH host form.

add-device-title = Add Device
add-device-close = Close
# The rail's entries. Cloud providers and WSL keep their own names.
add-device-source-host = SSH host
add-device-source-catalog = Add provider
# The heading over the rail's cloud providers.
add-device-cloud-heading = CLOUD
# The device picker's line under "Add Device…": what this build can add.
add-device-offer-ssh-wsl-cloud = An SSH host, a WSL distribution, or a cloud machine
add-device-offer-ssh-wsl = An SSH host or a WSL distribution
add-device-offer-ssh-cloud = An SSH host or a cloud machine
add-device-offer-ssh = Set up a remote host over SSH
add-device-coder-connect = Connect a Coder deployment in Settings, then create or attach workspaces here.
add-device-open-settings = Open Settings…
# The Add provider pane's lead, over tiles of providers not set up yet.
add-device-providers-all-set = Every cloud provider is set up and listed beside SSH.
add-device-providers-lead = Set up a provider in Settings, and Add Device lists it beside SSH.

# The SSH form. $target is what the user typed, such as user@host or an
# SSH alias; $user is the login SSH resolved for it.
add-device-target-placeholder = user@hostname or SSH alias
add-device-label-placeholder = The SSH target
add-device-intro = Herdr checks the host over SSH and saves the device. If Herdr is missing or SSH needs your input, setup continues in a local workspace.
add-device-target = SSH target
add-device-label = Label (optional)
add-device-session = Remote session (optional)
add-device-checking = Checking Herdr on { $target }…
add-device-starting = Herdr is installed on { $target } but not running. Starting it…
add-device-saving = Herdr is running on { $target }. Saving the device…
add-device-fetching-key = Fetching the host key of { $target }…
add-device-verifying = Checking { $target } is not saved yet…
add-device-opening = Opening a local workspace…
add-device-cancel = Cancel
add-device-trust-key = Trust key
add-device-submit = Add device
add-device-submit-checking = Checking…
add-device-check-failed = Check { $target }: { $error }
add-device-ssh-failed = SSH to { $target }: { $error }
add-device-open-failed = Open a local workspace: { $error }
add-device-unexpected-response = Unexpected daemon response

# macOS's Local Network permission, which an SSH check can be refused.
add-device-local-network-settings = Open Local Network Settings
add-device-local-network-waiting = macOS refused Herdr's connection to { $target } on your local network. Allow Herdr when macOS asks, or turn it on in System Settings › Privacy & Security › Local Network. Herdr checks again by itself.

# Approving an SSH host's key. $known_hosts is the file the key is saved to,
# and $host_file the key's own file on the host.
add-device-key-changed = The host key of { $target } differs from the one you trusted before. Someone could be intercepting the connection; check it in a terminal.
add-device-key-fetch-failed = Fetch the host key of { $target }: { $error }
add-device-key-unknown = { $target } is not a known host yet. Trust it only if this fingerprint matches the host's own key:
add-device-key-check = On the host, ssh-keygen -lf { $host_file } prints it. Trusting adds the key to { $known_hosts }.

# What setup offers when the host check finds Herdr missing, old, or SSH
# needing the user.
add-device-ask-install = Herdr was not detected on { $target }. Should we install it?
add-device-ask-update = The Herdr on { $target } is too old for this app. Should we update it?
add-device-ask-terminal = Setting up { $target } needs your input, such as an SSH password or host key. Continue in a local terminal?
add-device-ask-restart = Restart the Herdr server on { $target }? A local terminal asks you to confirm.
# $example is the target with a placeholder login, such as name@host.
add-device-ask-key-user = { $target } accepted none of your SSH keys for the user { $user }. If you log in there as someone else, change the target to { $example } and press Enter to check it. Otherwise, install your key for { $user }? A local terminal runs ssh-copy-id, which asks for the password once, then saves the device.
add-device-ask-key = { $target } accepted none of your SSH keys. Install your key there? A local terminal runs ssh-copy-id, which asks for the password once, then saves the device.
# The placeholder login in $example above.
add-device-example-user = name
add-device-install = Install
add-device-update = Update
add-device-open-terminal = Open terminal
add-device-set-up-key = Set up key login
# Windows runs no setup in a local terminal yet, so these say which command
# to run in one.
add-device-terminal-install = Setting up { $target } needs a terminal: Herdr was not detected there. Run `herdr machine add { $target }` in a terminal, then add the device again.
add-device-terminal-update = Setting up { $target } needs a terminal: its Herdr is too old for this app. Run `herdr machine add { $target }` in a terminal, then add the device again.
add-device-terminal-input = Setting up { $target } needs a terminal: SSH needs your input, such as a password or host key. Run `herdr machine add { $target }` in a terminal, then add the device again.
add-device-terminal-restart = Setting up { $target } needs a terminal: its Herdr server has to restart. Run `herdr machine add { $target }` in a terminal, then add the device again.
add-device-terminal-key-user = Setting up { $target } needs a terminal: it accepted none of your SSH keys for the user { $user } (use { $example } to log in as someone else). Run `herdr machine add { $target }` in a terminal, then add the device again.
add-device-terminal-key = Setting up { $target } needs a terminal: it accepted none of your SSH keys. Run `herdr machine add { $target }` in a terminal, then add the device again.
# The SSH host form's opt-in to connect without verifying the host's key.
add-device-skip-host-key = Skip host key verification (dangerous)
# Shown while the opt-in is on. Coder is a product name; keep it as it is.
add-device-skip-host-key-warning = Herdr will accept any key this host offers, now and on every connection. Anyone who can intercept the connection can impersonate the host and read your terminals. Use it only for hosts that keep no key, such as Coder workspaces.
