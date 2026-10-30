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
