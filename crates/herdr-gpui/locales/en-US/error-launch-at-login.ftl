# Why launch at login could not be turned on or off (Windows).
# herdr-gpui-launcher.exe is a file name.

# $path is where the launcher should be.
error-launch-at-login-no-launcher = Launch at login needs herdr-gpui-launcher.exe beside the app, at { $path }
error-launch-at-login-current-exe = Could not find the running executable
# $operation is what failed on the startup entry in the registry.
error-launch-at-login-registry = { $operation ->
    [open] Could not open the startup entry
    [read] Could not read the startup entry
    [write] Could not write the startup entry
   *[remove] Could not remove the startup entry
}
