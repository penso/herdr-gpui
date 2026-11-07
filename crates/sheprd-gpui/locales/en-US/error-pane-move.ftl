# Why a pane or tab could not be moved.

error-pane-move-unsupported-host = Moving panes needs a local session or SSH host; custom socket endpoints are unsupported.
error-pane-move-development-session = Moving panes is not available for development (--dev) sessions.
# $id is a Herdr pane, tab, or workspace ID, quoted.
error-pane-move-invalid-id = Herdr ID { $id } cannot be passed to the herdr CLI.
error-pane-move-unchanged-same-tab = The pane is already in that tab.
error-pane-move-unchanged-zoomed-tab = A zoomed tab cannot give or take panes. Unzoom it and try again.
error-pane-move-unchanged-other = Herdr left the pane where it was.
# $error is Herdr's own message.
error-pane-move-refused = Could not move the pane: { $error }
error-pane-move-missing-cli = Could not move the pane: the herdr CLI was not found on this host.
error-pane-move-script = Could not move the pane: { $error }
error-pane-move-decode = Could not move the pane: herdr returned unexpected output.
# $pane is a Herdr pane ID.
error-pane-move-partial-tab-move = Could not move pane { $pane } into the created tab: { $error }
error-pane-move-rename = Moved the tab, but could not keep its name: { $error }
