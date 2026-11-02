# Agent chats: the connection to herdr-acp, the chat program in a pane, and
# starting a new chat. [chat.env] and [chat.mirrors] are config tables.

error-chat-apply = a chat change does not fit the transcript
error-chat-busy = too many chat commands are waiting for an answer
# $reason is the chat program's reason for leaving, as its protocol names it.
error-chat-bye = the chat program left: { $reason }
error-chat-closed = the chat program closed the connection
error-chat-connect = could not reach the chat program
# $name is the variable's name as the config writes it.
error-chat-environment-name = `{ $name }` cannot be a chat environment variable
# $count is the most variables, and $bytes the most bytes, they may hold.
error-chat-environment-size = [chat.env] holds more than { $count } variables or { $bytes } bytes
error-chat-handshake = the chat program did not greet first
error-chat-mirror-config = [chat.mirrors] needs at most 16 valid pane and endpoint identifiers
error-chat-no-endpoint-dir = no private directory holds chat sockets
error-chat-not-attached = the chat view is not attached
error-chat-overflow = the chat view fell behind the program
error-chat-read = could not read from the chat program
error-chat-read-only = This is a read-only transcript; use the agent's terminal to reply
error-chat-remote = could not reach the chat program over SSH
error-chat-spawn = could not start the chat worker
# $got and $after are positions in the chat program's stream of changes.
error-chat-stream = chat change { $got } does not follow { $after }
# $version is a protocol version number.
error-chat-version = the chat program speaks protocol version { $version }

# $agent is an agent's name, such as Claude Code.
error-chat-start-agent-missing = { $agent } is not installed on this host
error-chat-start-disabled = ACP chats are off in Settings
error-chat-start-no-workspace = no workspace is focused
error-chat-start-not-connected = Herdr UI is not connected
error-chat-start-not-local = chats start on this machine's Herdr or an SSH host's
error-chat-start-program-missing = herdr-acp was not found beside Herdr UI or on PATH
error-chat-start-program-unknown = Herdr UI is still looking for herdr-acp
error-chat-start-response = Herdr's answer did not name the chat's tab
error-chat-start-slot-busy = another tab is still opening
error-chat-start-timeout = Herdr did not open the chat's tab in time

# A picture pasted into a chat's composer, saved to a file to attach.
error-chat-pasted-folder = no private folder holds pasted images
error-chat-pasted-write = could not save the pasted image

# A web page an agent wrote, copied to a private file to show in its chat.
error-chat-page-folder = no folder for pages
error-chat-page-not-file = not a file path
error-chat-page-too-large = the page is too large

# A picture, diagram, or formula in a chat message that could not be drawn.
error-chat-visual-base64 = the image is not valid base64
error-chat-visual-decode = the image could not be decoded
error-chat-visual-diagram = the diagram could not be read
error-chat-visual-fetch = the picture could not be fetched
# $reason is the formula renderer's own reason, in English.
error-chat-visual-math = the formula could not be drawn: { $reason }
error-chat-visual-math-limit = the formula exceeds the rendering limits
error-chat-visual-math-parse = the formula could not be parsed
error-chat-visual-not-an-image = the address does not answer with a picture
error-chat-visual-panicked = the diagram renderer failed
error-chat-visual-raster = the diagram could not be drawn

# Why the chat program did nothing with a command the window sent it.
error-chat-rejection-empty-prompt = the prompt is empty
error-chat-rejection-prompt-too-long = the prompt is longer than the program accepts
error-chat-rejection-not-ready = the agent cannot take prompts
error-chat-rejection-not-working = no turn is running
error-chat-rejection-unknown-permission = that permission request was already answered
error-chat-rejection-unknown-option = that is not one of the request's options
error-chat-rejection-unknown-mode = the agent has no such mode
error-chat-rejection-unknown-setting = the agent has no such setting, or it does not take that value
error-chat-rejection-unknown-queued = that prompt is no longer queued
error-chat-rejection-agent-unavailable = the program could not deliver the command to the agent
# $count is the most files one prompt takes.
error-chat-rejection-too-many-attachments = a prompt takes at most { $count } files
error-chat-rejection-queue-full = too many prompts are waiting; wait for the turn to end
error-chat-rejection-unknown-file = an attached file does not exist or cannot be read
error-chat-rejection-unsupported = this chat program is older than Herdr UI; start a new chat to use that
error-chat-rejection-other = the program refused the command

# An interactive card an agent's answer holds, whose JSON the window checks.
error-chat-card-action-gone = invalid interactive card: action is no longer present
error-chat-card-component = invalid interactive card: component fields or bounds
error-chat-card-component-count = invalid interactive card: component count
error-chat-card-field-id = invalid interactive card: field ids must be unique ASCII names
error-chat-card-form-value = invalid interactive card: form value
error-chat-card-json = invalid interactive card JSON
error-chat-card-title = invalid interactive card: title
error-chat-card-too-large = the interactive card exceeds its size limit
error-chat-card-too-many-fields = invalid interactive card: too many editable fields
