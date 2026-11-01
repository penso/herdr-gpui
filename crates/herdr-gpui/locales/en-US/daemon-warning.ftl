# The notice shown when the local daemon's socket fails the checks the app
# makes before it reads local Git and pull request details. Each body says
# what failed, then how to fix it; "the GUI" is this app. When $command is
# "yes", a chmod command line follows the body on its own line.

daemon-warning-title = Local Git and PR details unavailable
daemon-warning-peer-user = The daemon's user could not be verified as your user. Run the local daemon as your user, then reconnect the GUI.
daemon-warning-not-socket = The standard local endpoint is missing or is not a socket. Connect to the standard local session to enable Git and PR details.
daemon-warning-other-socket = This is not the standard local session socket. Connect to the standard local session to enable Git and PR details.
daemon-warning-socket-permissions =
    { $command ->
        [yes] The daemon socket has untrusted ownership or write permissions. Check ownership and remove group/other write access, then reconnect the GUI.
       *[no] The daemon socket has untrusted ownership or write permissions. Make it owned by your user, remove group/other write access, then reconnect the GUI.
    }
daemon-warning-directory-permissions =
    { $command ->
        [yes] The daemon socket directory must be owned by your user and not world-writable. Check ownership and remove world-write access, then reconnect the GUI.
       *[no] The daemon socket directory must be owned by your user and not world-writable. Make it owned by your user, remove world-write access, then reconnect the GUI.
    }
