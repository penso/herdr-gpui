# Azure VMs and Container Apps sandboxes as cloud devices. $detail is
# Azure's own text (or the Azure CLI's), kept as it is; $error is the
# failure underneath.

error-remote-azure-missing-subscription = Azure needs a subscription ID. Configure one in Settings > Cloud Devices.
error-remote-azure-missing-credentials = Azure needs credentials (client secret or Azure CLI login).
error-remote-azure-authentication = Azure authentication failed: { $detail }
error-remote-azure-forbidden = Azure denied access: { $detail }
# $code is the HTTP status code, such as 500.
error-remote-azure-status = Azure API returned HTTP { $code }: { $detail }
error-remote-azure-network = Azure network request failed or timed out: { $error }
error-remote-azure-read = Azure response could not be read within the size/time limit.
error-remote-azure-size = Azure response exceeded the size limit.
error-remote-azure-json = Invalid Azure JSON response.
error-remote-azure-header = Invalid Azure HTTP header.
error-remote-azure-vm = Azure VM failed: { $detail }
error-remote-azure-deleted = The Azure VM was deleted.
error-remote-azure-start-timeout = The Azure VM did not become ready within 10 minutes.
error-remote-azure-sandbox = Azure sandbox failed: { $detail }
error-remote-azure-sandbox-deleted = The Azure sandbox was deleted.
error-remote-azure-sandbox-timeout = The Azure sandbox did not become ready within 5 minutes.
error-remote-azure-delete-timeout = Azure did not finish deleting the machine within 10 minutes.
error-remote-azure-stream = The Azure sandbox shell stream failed.
error-remote-azure-stream-header = Invalid Azure sandbox shell stream header.
error-remote-azure-stream-closed = The Azure sandbox shell stream closed before the command finished.
error-remote-azure-stream-message = The Azure sandbox shell stream sent data this app does not understand.
error-remote-azure-tls = Could not set up TLS for the Azure sandbox shell stream.
error-remote-azure-executable = Could not find this app's executable to relay the sandbox shell.
error-remote-azure-cancelled = Cancelled.
error-remote-azure-gateway = Azure returned SSH connection data this app does not understand.
error-remote-azure-ssh-missing = Azure devices need ssh and ssh-keygen on the PATH; one could not be run.
error-remote-azure-ssh-keygen = Generating the Azure device SSH key failed.
error-remote-azure-az-missing = Azure CLI (az) was not found on PATH.
error-remote-azure-az-failed = Azure CLI command failed: { $detail }
error-remote-azure-process = Could not run the command that reaches the machine.
error-remote-azure-storage = Could not update saved Azure credentials: { $error }
# $id is the machine's ID at Azure.
error-remote-azure-save-device = Could not save cloud device { $id }: { $error }
error-remote-azure-account = The Azure account in Settings disagrees with this saved device.
error-remote-azure-not-running = The Azure machine is not running; connecting to it starts it.
# Azure has none of the machine size asked for, at that priority or region.
error-remote-azure-sku-not-available = Azure has no capacity for this size: { $detail }
