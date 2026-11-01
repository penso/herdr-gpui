# What the main window says about input it could not deliver to a pane:
# typed keys, the mouse, pastes, dropped files, and images, and the file
# copy to an SSH device that a drop or paste starts. Most show on the
# status line; $error says why, in the error's own words.

# The terminal area's name for screen readers.
window-input-terminal = Terminal

## Typed keys and held input

window-input-failed = Input not sent: { $error }
window-input-not-connected = Not connected: input not sent
# The window holds input while a pane redraws; the hold filled up.
window-input-stopped = Input not sent: the terminal stopped updating
window-input-focus-changed = Input not sent: focus changed before it arrived
window-input-still-copying = Input not sent: the files are still copying
window-input-refused = Input not sent
# Held input dropped after one that did not go out. $reason is one of the
# lines above, or another error; $count counts what was dropped after it.
window-input-discarded =
    { $count ->
        [one] { $reason } (1 held input discarded)
       *[other] { $reason } ({ $count } held inputs discarded)
    }
window-input-accessibility-failed = Accessibility input not sent: { $error }

## The mouse

window-input-wheel-failed = Wheel input not sent: { $error }
window-input-mouse-failed = Mouse input not sent: { $error }
window-input-mouse-release-failed = Mouse release not sent: { $error }
window-input-scroll-failed = Scroll not sent: { $error }
window-input-pane-resize-failed = Pane resize not sent: { $error }

## Pastes, dropped files, and images

window-input-paste-menu = Paste not sent: a menu is open
window-input-paste-focus = Paste not sent: the window was taking focus
window-input-files-menu = Files not pasted: a menu is open
window-input-files-nowhere = Files not pasted: nothing under the drop point
window-input-files-cloud = Files not pasted: file drops are not supported on cloud devices yet.
window-input-files-busy = Files not pasted: another copy is running
window-input-files-failed = Files not pasted: { $error }
# A pasted or dropped image, or a paste whose clipboard is read in the
# background, that could not be prepared or sent. $kind is "image" or
# "paste".
window-input-prepared-failed =
    { $kind ->
        [image] Image not sent: { $error }
       *[paste] Paste not sent: { $error }
    }
# The notice's title and body for the same failure.
window-input-prepared-discarded =
    { $kind ->
        [image] Image discarded
       *[paste] Paste discarded
    }
window-input-prepared-discarded-body = { $error } Nothing was uploaded or pasted.
window-input-image-resized = Image resized for upload
window-input-image-resized-body = The image was recompressed or downscaled to 16 MiB or less and queued for upload. The original is unchanged.

## Copying dropped files to an SSH device

# What is copying: one file's name, or this when it has none, or a count.
window-input-copy-file = File
window-input-copy-files =
    { $count ->
        [one] { $count } file
       *[other] { $count } files
    }
window-input-copying = Copying...
window-input-copy-finishing = Finalizing copy...
window-input-copy-cancelling = Cancelling...
# The card's button, and its label once pressed.
window-input-copy-cancel = Cancel
window-input-copy-cancel-pressed = Cancelling
# $sent and $total are sizes as window-input-size writes them; $percent is
# a whole number.
window-input-copy-progress = { $sent } / { $total } ({ $percent }%)
# A size in bytes: $value has at most one decimal, and $unit is b, kib,
# mib, gib, tib, pib, or eib (binary multiples).
window-input-size =
    { $unit ->
        [kib] { $value } KiB
        [mib] { $value } MiB
        [gib] { $value } GiB
        [tib] { $value } TiB
        [pib] { $value } PiB
        [eib] { $value } EiB
       *[b] { $value } B
    }
# Notices when a copy ends. $device names the device the files went to.
window-input-copy-not-started = Copy not started
window-input-copy-not-started-body = Another file copy is still running. Cancel it or wait for it to finish.
window-input-copy-complete = Copy complete
window-input-copy-complete-body = Remote file paths pasted. Files remain in the remote temporary directory until removed.
window-input-copy-cancelled = Copy cancelled
window-input-copy-failed = Copy failed
window-input-cleanup-failed = Remote cleanup failed
window-input-cleanup-failed-body = No path was pasted. Temporary files may remain on the original host ({ $device }).

## Other gestures

# Dragging a tab to another place in the strip.
window-input-tab-move-failed = Tab move not sent: { $error }
# Clicking a link whose handler could not open it.
window-input-link-failed = Link handler failed: { $error }
# The window resized, and the pane size could not be sent.
window-input-resize-failed = Resize: { $error }
