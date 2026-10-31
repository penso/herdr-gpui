# Boat sandboxes as cloud devices. $detail is Boat's own text, kept as it
# is; $error is the failure underneath; $id is a sandbox's ID at Boat.

# $field names a setting, such as boat.api_url.
error-remote-boat-url = { $field } must be https (http only on loopback) with no credentials, query, or fragment.
error-remote-boat-missing-key = Boat needs an API key. Save one in Settings > Cloud devices.
error-remote-boat-authentication = Boat rejected the API key.
error-remote-boat-forbidden = Boat denied access. Check the API key's scopes and sandbox access.
# $code is the HTTP status code, such as 500.
error-remote-boat-status = Boat request failed (HTTP { $code })
error-remote-boat-status-message = Boat request failed (HTTP { $code }): { $detail }
error-remote-boat-network = Boat network request failed or timed out.
error-remote-boat-read = Boat response could not be read within the size/time limit.
error-remote-boat-size = Boat response exceeded the size limit.
error-remote-boat-json = Invalid Boat JSON response.
error-remote-boat-header = Invalid Boat HTTP header.
error-remote-boat-sandbox = Boat sandbox failed: { $detail }
error-remote-boat-deleted = The Boat sandbox was deleted.
error-remote-boat-start-timeout = The Boat sandbox did not start within 10 minutes.
error-remote-boat-cancelled = Cancelled.
# $path is the file that keeps a creation request until it finishes.
error-remote-boat-pending-io = Could not save or read the pending Boat creation at { $path }: { $error }
error-remote-boat-pending-busy = Another Herdr UI process is creating or recovering a Boat sandbox for this account.
error-remote-boat-pending-expired = The pending Boat creation is too old to retry safely. Check the Boat dashboard for its sandbox before forgetting the pending request in Settings > Cloud devices > Boat.
error-remote-boat-pending-creation = Boat creation may have succeeded. Use Recover creation in Settings > Cloud devices > Boat to retry the saved request: { $error }
error-remote-boat-clock = Could not read the clock for Boat creation recovery: { $error }
error-remote-boat-gateway = Boat returned SSH connection data this app does not understand.
error-remote-boat-ssh-missing = Boat devices need ssh and ssh-keygen on the PATH; one could not be run.
error-remote-boat-ssh-key = Could not create or read the Boat SSH key.
error-remote-boat-ssh-keygen = ssh-keygen could not create the Boat SSH key.
error-remote-boat-gateway-file = Could not prepare the SSH command for Boat.
error-remote-boat-save-device = Boat sandbox { $id } exists, but its device could not be saved: { $error }. Keep this ID to recover the sandbox.
error-remote-boat-setup = Boat sandbox { $id } is saved. Use Finish setup in Settings > Cloud devices > Boat to retry: { $error }
# [boat] is the config table.
error-remote-boat-account = This device belongs to a Boat account that is no longer configured in [boat].
error-remote-boat-key-file = Could not load the Boat SSH private key: { $error }
