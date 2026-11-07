# The Git check in the close workspace dialog: whether closing would leave
# work behind. Each sentence shows only when it applies.

workspace-close-checking = Checking for uncommitted files and unpushed commits...
workspace-close-dirty = Uncommitted files are present.
workspace-close-unpushed = Unpushed commits are present.
workspace-close-unknown = Git status could not be verified for every checkout.
workspace-close-clean = No uncommitted files or unpushed commits found (using local remote-tracking refs).
# $word is what the user types to agree, "close" in every language; keep it
# as it is. "Cancel" names the dialog's Cancel button.
workspace-close-consent = Type { $word } to consent to closing anyway, or Cancel to keep working.
