# Why VS Code did not run on an SSH host, for that host's workspaces. The
# "command line" is VS Code's own `code` program, which the app downloads.

error-code-server-host-download = Could not download VS Code's command line on the host.
# curl and wget are program names.
error-code-server-host-no-downloader = The host has neither curl nor wget to download VS Code's command line.
# { $name } is the address the app loads the host's VS Code at, such as
# vscode-0123456789abcdef.localhost.
error-code-server-host-not-loopback = This computer resolves { $name } to another machine, so the host's VS Code token is not sent there.
error-code-server-host-setup = Could not prepare VS Code's folder in the host's home folder.
error-code-server-host-ssh = SSH could not reach the host, or could not forward VS Code's port.
error-code-server-host-unpack = Could not unpack VS Code's command line on the host.
error-code-server-host-unsupported = VS Code's command line is not made for this host's system or processor.
