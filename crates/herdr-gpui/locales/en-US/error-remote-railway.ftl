# Railway VMs as cloud devices, reached with an SSH key this app creates or
# the person picks. $path is the SSH private key's file. $detail is Railway's
# own text, kept as it is.

error-remote-railway-keygen-missing = Railway devices need ssh-keygen on the PATH; it could not be run.
# $status is how ssh-keygen exited, as the program reports it: Some(1), or
# None when a signal stopped it.
error-remote-railway-keygen = ssh-keygen could not create or read the key (exit status { $status }).
error-remote-railway-keygen-timeout = ssh-keygen did not finish within 30 seconds.
error-remote-railway-unusable-key = { $path } is not a usable SSH private key. A passphrase-protected key needs its .pub file next to it and must be loaded in your SSH agent.
error-remote-railway-key-name = Use 1 to 64 letters, digits, dots, underscores or hyphens, not starting with a dot or hyphen and not ending in .pub.
error-remote-railway-key-exists = { $path } already exists, and Herdr never replaces a key. Choose another name.
error-remote-railway-key-path = Choose an absolute path to an SSH private key.
error-remote-railway-key-file = Could not use the key file { $path }.
# $path is the file that keeps the saved Railway devices.
error-remote-railway-store = Could not update { $path }.
error-remote-railway-no-identity = This device has no saved key path. Remove it and set it up again.
error-remote-railway-key-missing = The SSH key for this device is missing: { $path }. It is never replaced with a new one; restore it, or remove the device.
error-remote-railway-key-changed = The key at { $path } is not the one this device was set up with, so it was not used. Restore that key, or remove the device and set it up again.
error-remote-railway-public-key-mismatch = The public key beside { $path } does not match its private key. Restore the matching .pub file before connecting.
error-remote-railway-terminal = Could not open a terminal to create the Railway VM.
error-remote-railway-terminal-unsupported = Creating the VM needs a terminal, which this build of Herdr cannot open.
error-remote-railway-create-timeout = Railway did not finish creating the VM within 90 seconds.
error-remote-railway-create-refused = Railway did not create a VM for this key: { $detail }
# railway.new is Railway's web address.
error-remote-railway-unreachable = Could not reach railway.new: { $detail }
