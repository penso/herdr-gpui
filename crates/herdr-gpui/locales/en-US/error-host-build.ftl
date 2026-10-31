# Copying one of this app's programs (herdr-automate, herdr-remote-control)
# to a host. $program is the program's name.

# $path is the program file this app ships.
error-host-build-binary = Could not read the { $program } build at { $path }
error-host-build-checksum = The copy on the host did not match this app's build; nothing was installed
# $output is what `uname -sm` printed.
error-host-build-platform = Cannot tell this host's platform from `uname -sm`: { $output }
