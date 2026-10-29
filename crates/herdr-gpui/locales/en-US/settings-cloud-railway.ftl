# Settings > Cloud devices > Railway: no account, only the SSH key new VMs
# are created with. Paths, key names, and machine names stay as they are.

settings-cloud-railway-note = Temporary VM from railway.new, created with the SSH key below.
settings-cloud-railway-unavailable = Cloud devices are unavailable on Windows.
settings-cloud-railway-key = SSH key
settings-cloud-railway-source-generate = Generate a new key
settings-cloud-railway-source-existing = Use an existing key
settings-cloud-railway-key-name = Key name
# What a key's name may be; .pub is the public key's file extension.
settings-cloud-railway-name-rule = Use 1 to 64 letters, digits, dots, underscores or hyphens, not starting with a dot or hyphen and not ending in .pub.
# Where a generated key goes: $folder is a folder, $name the key's file name.
settings-cloud-railway-key-location = Creates { $folder }/{ $name } and { $name }.pub.
settings-cloud-railway-key-location-no-ssh = No ~/.ssh folder, so this creates { $folder }/{ $name } and { $name }.pub.
settings-cloud-railway-key-location-checking = Checking where the key goes…
settings-cloud-railway-generate = Generate key
settings-cloud-railway-key-created = Created { $path }.
settings-cloud-railway-key-path = Private key path
settings-cloud-railway-key-path-note = The file is never changed or deleted. A passphrase-protected key needs its .pub file beside it and a loaded SSH agent.
settings-cloud-railway-browse = Browse…
# The title of the system file dialog.
settings-cloud-railway-choose-key = Choose SSH private key
settings-cloud-railway-install = Install Herdr on the VM when missing
settings-cloud-railway-create = Connect and set up
settings-cloud-railway-busy = Wait for the current Railway action to finish.
settings-cloud-railway-key-needed = Generate a key, or enter the path of an existing one.
settings-cloud-railway-connecting = Connecting { $name }. The device picker shows its progress.
settings-cloud-railway-no-window = Open a main Herdr window to set up a VM.

## Railway devices: each one's machine, then the key it uses.

settings-cloud-railway-devices = Railway devices
settings-cloud-railway-device = { $machine } · { $key }
settings-cloud-railway-device-key-missing = { $machine } · { $key } · key file missing
