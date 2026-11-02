# Failures reaching or managing a WSL (Windows Subsystem for Linux)
# distribution. wsl.exe and wslpath are Windows commands. $path is a file or
# folder dropped on a WSL terminal.

error-client-wsl-catalog = unsupported WSL device list version, size, or entries
error-client-wsl-catalog-schema = invalid WSL device list schema
error-client-wsl-closed = WSL bridge closed; check that the distribution starts and Herdr is installed in it
# $status is how wsl.exe exited, in the system's words.
error-client-wsl-command = wsl.exe failed ({ $status })
error-client-wsl-host-exists = this WSL distribution is already a device
error-client-wsl-host-missing = this WSL distribution is not a saved device
error-client-wsl-invalid-distro = invalid WSL distribution name
error-client-wsl-path-invalid = { $path } is not an absolute Windows path a WSL distribution can map
error-client-wsl-path-limit = a WSL distribution takes at most 256 dropped paths at once
error-client-wsl-path-response = invalid answer from wslpath
error-client-wsl-path-unmapped = { $path } is not reachable from this WSL distribution; only its own files and mounted drives are
error-client-wsl-timeout = WSL did not answer in time; the distribution may be stopped or waiting for setup
error-client-wsl-unsupported = WSL distributions are only available on Windows
