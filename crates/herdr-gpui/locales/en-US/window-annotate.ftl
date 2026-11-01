# Annotating terminal text: the note field over a pane's selection, and
# the offer to annotate it.

# The offer beside a selection.
window-annotate-offer = Annotate
window-annotate-title = Note to agent
window-annotate-placeholder = Message to agent…
# The buttons: add the note to the queue, or send it with those queued.
window-annotate-queue = Queue
window-annotate-send = Send
# The hint beside them: $count notes wait; or the note is copied, when no
# agent is in the pane; or Enter sends it.
window-annotate-hint-queued = { $count } queued · Enter to send
window-annotate-hint-copies = Copies to clipboard
window-annotate-hint-send = Enter to send

## Flashes

window-annotate-select = Select terminal text to annotate it
window-annotate-pane-only = Only text in a pane can be annotated
window-annotate-offscreen = Scroll so the whole selection shows, then annotate it
window-annotate-failed = Cannot annotate: { $error }
window-annotate-empty = Write a note before adding it
# $count is how many notes may wait at most.
window-annotate-full =
    { $count ->
        [one] { $count } note is queued; clear the field and press Enter to send it
       *[other] { $count } notes are queued; clear the field and press Enter to send them
    }
window-annotate-queued =
    { $count ->
        [one] { $count } note queued; Enter on the next one sends them
       *[other] { $count } notes queued; Enter on the next one sends them
    }
window-annotate-stale-copied = Notes whose original daemon is not selected were copied
