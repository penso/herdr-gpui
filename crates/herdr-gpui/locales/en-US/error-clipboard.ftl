# Why the clipboard could not be read for a paste.

error-clipboard-changed = The clipboard changed while reading. Try pasting again.
error-clipboard-encoding = Clipboard text is not valid UTF-8.
error-clipboard-process = Could not read the clipboard; on Linux, install wl-clipboard for Wayland or xclip for X11.
# $limit is a number of bytes.
error-clipboard-size = Clipboard content exceeds the { $limit }-byte limit.
error-clipboard-timeout = Clipboard acquisition timed out.
error-clipboard-unsupported = Remote clipboard acquisition is not supported on this platform.

# macOS only: the helper process Herdr starts to read the pasteboard.
# $status is how the helper exited, as the system words it.
error-clipboard-macos-exited = The macOS clipboard reader exited unsuccessfully ({ $status }). Try pasting again.
error-clipboard-macos-missing = Herdr's app was removed or replaced since it started, so it cannot read the clipboard. Restart Herdr to paste.
error-clipboard-macos-process = Could not run the macOS clipboard reader
error-clipboard-macos-response = The macOS clipboard reader returned an invalid response
error-clipboard-macos-text-encoding = Clipboard text is not valid UTF-8.
