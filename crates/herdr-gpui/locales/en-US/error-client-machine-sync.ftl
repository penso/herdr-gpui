# Failures syncing the list of saved machines between the user's computers.

error-client-machine-sync-authorized-keys = Could not update this computer's authorized SSH keys (~/.ssh/authorized_keys)
error-client-machine-sync-catalog = The machine's saved devices could not be read
error-client-machine-sync-changed = The machine's saved devices changed during the sync; it is synced next time
error-client-machine-sync-home = This computer has no home folder for its SSH key
# $status is how ssh-keygen exited, in the system's words.
error-client-machine-sync-keygen = ssh-keygen could not create the herdr SSH key ({ $status })
error-client-machine-sync-keygen-spawn = ssh-keygen could not be run; install OpenSSH
error-client-machine-sync-key-read = The herdr SSH key (~/.ssh/herdr_ed25519) could not be read
error-client-machine-sync-public-key = The machine has a herdr SSH key that is not an Ed25519 key
error-client-machine-sync-reply = The machine answered in a form machine sync does not understand
error-client-machine-sync-state-schema = The machine sync settings could not be read
error-client-machine-sync-state-value = The machine sync settings are not valid; check this computer's address and name
