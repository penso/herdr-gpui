# The setup script a new cloud machine runs. [cloud] is a config table, and
# agents and setup are its keys.

# $device is a cloud device's name.
error-cloud-setup-no-device = No cloud device { $device } in this window
# $provider is a cloud provider's name.
error-cloud-setup-no-script = No setup script is configured for { $provider } devices; set [cloud] agents or setup
error-cloud-setup-script-nul = The cloud device setup script contains a NUL byte
# $limit is a size in bytes.
error-cloud-setup-script-too-long = The cloud device setup script is longer than { $limit } bytes
