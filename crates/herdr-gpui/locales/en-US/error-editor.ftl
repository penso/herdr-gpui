# Opening a file in the editor pane, a terminal editor such as Neovim.
# editor_command is a config key; nvim is a program's name.

error-editor-busy = Another file is still opening in the editor
error-editor-command = editor_command must be at most 1024 bytes without quotes, backslashes, or control characters
error-editor-no-pane = No local pane to open the editor beside
error-editor-path = This path cannot be typed into a shell safely, so it opens in the default app
error-editor-remote = The Neovim in the editor pane did not answer
error-editor-remote-busy = The Neovim in the editor pane is busy: answer it, then try again
error-editor-remote-failed = The Neovim in the editor pane could not open the file
error-editor-remote-launch = Could not run nvim to reach the editor pane
error-editor-response = Unexpected daemon response while opening the editor pane
error-editor-unsupported = Opening a terminal editor needs a Unix shell in the pane
