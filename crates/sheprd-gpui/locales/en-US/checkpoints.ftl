# The checkpoints dialog of a workspace row: the checkout's snapshots, and
# restoring one.

checkpoints-title = Checkpoints
# The workspace and the checkout's branch, under the title.
checkpoints-subtitle = { $workspace } · { $branch }
# $label is the checkpoint's name; $age says how long ago it was taken, such
# as "3 min ago", in English for now.
checkpoints-restore-title = Restore “{ $label }” from { $age }?
checkpoints-restore-note = Files in this checkout go back to how they were then, and { $branch } moves back to the commit it was on. Files created since are removed; ignored files are left alone.
checkpoints-undo-note = The current state is saved as a checkpoint first, so this can be undone.
checkpoints-agent-working = An agent in this workspace is still working and may keep changing files.
checkpoints-loading = Loading checkpoints...
checkpoints-empty = No checkpoints yet. One is taken each time an agent here starts or finishes a turn.
checkpoints-off = No checkpoints. Checkpoint agent turns is off in Settings > General.
checkpoints-cancel = Cancel
checkpoints-close = Close
checkpoints-restore = Restore
checkpoints-restoring = Restoring...
# How many files a checkpoint changed, kept short beside +additions and
# −deletions.
checkpoints-files = { $count }f
# The day heading over yesterday's checkpoints; $date is the month and day.
checkpoints-yesterday = Yesterday, { $date }
