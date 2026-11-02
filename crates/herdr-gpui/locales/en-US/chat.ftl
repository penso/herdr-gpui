# Agent chats: the layouts they draw in, where the new-chat launcher sits,
# and everything a chat pane shows.

chat-layout-timeline = Timeline
chat-layout-bubbles = Bubbles
chat-layout-terminal = Terminal
chat-layout-document = Document
chat-layout-cards = Cards
chat-layout-imessage = iMessage
chat-layout-ia = iA
chat-launcher-plus = On + hover
chat-launcher-beside = After +
chat-launcher-leading = Before tabs
chat-launcher-trailing = Right end
chat-launcher-button = One button
chat-launcher-off = Off

# A chat pane's header: the agent's state, the clients watching it, and the
# button back to the chat from the agent's own terminal.
chat-status-starting = Starting
chat-status-idle = Idle
chat-status-working = Working
chat-status-failed = Failed
chat-status-exited = Exited
# $status is one of the states above; $elapsed how long the turn has
# taken, such as "12s".
chat-status-elapsed = { $status } { $elapsed }
chat-needs-input = Needs input
# $count is how many windows show this chat.
chat-watching = { $count } watching
chat-reconnecting = Reconnecting…
chat-show-chat = Chat

# The composer.
chat-composer-placeholder = Message the agent — / for commands, Shift-Enter for a new line
chat-composer-closed = The agent cannot take prompts
chat-read-only-hint = Continue this conversation in its terminal.
chat-reply-in-terminal = Reply in terminal
# Files copying to an SSH host before the prompt is sent. $host is its name.
chat-copying-files = { $count ->
    [one] Copying a file to { $host }…
   *[other] Copying { $count } files to { $host }…
}
chat-preparing-pictures = { $count ->
    [one] Preparing a picture…
   *[other] Preparing { $count } pictures…
}
# Lines of a long draft above the ones shown.
chat-draft-earlier = { $count ->
    [one] { $count } earlier line
   *[other] { $count } earlier lines
}
# The file picker's button.
chat-files-attach = Attach
# $error says why, in the system's or the app's words.
chat-files-not-attached = Files not attached: { $error }
chat-image-not-attached = Image not attached: { $error }
chat-files-too-many = A prompt takes at most { $count } files
chat-files-copying = Files are still being copied; send once they are attached
chat-prompt-too-long = The prompt is longer than the agent accepts
chat-select-less = Select less text to copy
# $error is why the chat could not start.
chat-start-failed = The chat did not start: { $error }

# The agent's terminal input under a chat that is shown in its terminal.
chat-input-awaiting-fit = Activate to fit agent input
chat-input-blocked = Question or approval pending · original agent input
chat-input-ready = Agent input · same session
chat-input-fit = Fit input
chat-input-compact = Compact input
chat-input-expand = Expand input

# Who said what, and what the agent is doing.
chat-you = You
chat-delivered = Delivered
chat-typing = typing…
chat-queued = Queued
chat-working = Working
chat-working-writing = Writing
chat-working-thinking = Thinking
# What the agent is doing now: one of the words above, or the step it runs.
chat-working-doing = { $doing }…
chat-answer-cut = The rest of this answer is too long to show here; Copy takes all of it.

# Copy buttons and what their flash says.
chat-copy = Copy
chat-copy-message-done = Copied the message
chat-copy-answers-label = Copy answers
chat-copy-answers-done = Copied the answers
chat-copy-code-label = Copy code
chat-copy-code-done = Copied the code
chat-copy-latex-label = Copy LaTeX
chat-copy-latex-done = Copied the LaTeX

# A tool call's card: its kind, where it stands, and its output.
chat-tool-read = Read
chat-tool-edit = Edit
chat-tool-delete = Delete
chat-tool-move = Move
chat-tool-search = Search
chat-tool-run = Run
chat-tool-think = Think
chat-tool-subagent = Sub-agent
chat-tool-fetch = Fetch
chat-tool-mode = Mode
chat-tool-other = Tool
chat-tool-state-pending = pending
chat-tool-state-running = running
chat-tool-state-done = done
chat-tool-state-failed = failed
chat-tool-state-stopped = stopped
# A sub-agent's steps.
chat-tool-steps = { $count ->
    [one] { $count } step
   *[other] { $count } steps
}
# A tool that asks the user questions.
chat-tool-ask-user = Ask user
chat-tool-more-lines = { $count ->
    [one] { $count } more line
   *[other] { $count } more lines
}
chat-tool-terminal-elsewhere = Terminal output shows in the program's own view
# $id is the terminal's identifier.
chat-tool-terminal = Terminal { $id }: output is available in the program's terminal view.
# Labels on their own lines around a file's text before and after a change.
chat-diff-before = Before:
chat-diff-after = After:
chat-diff-new-file = New file:
# Items past the ones listed.
chat-more = and { $count } more

# The agent's answers to questions it asked, as Markdown.
chat-answers-preparing = Preparing answers…
chat-answers-record = Original record
# $number counts the questions from 1.
chat-question-number = Question { $number }
chat-question-answer = Answer
chat-question-truncated = … (answers truncated)

# Pages and pictures in a message.
chat-page-show = Show page
chat-page-open-in-browser = Open in browser
chat-page-hide = Hide
chat-page-html = HTML page
chat-page-html-lines = { $count ->
    [one] HTML page · { $count } line
   *[other] HTML page · { $count } lines
}
chat-page-opening = Opening the page…
chat-page-opened = Opened the page in your browser
# $host is an SSH host's name.
chat-page-remote = That page is on { $host }; open it there to see it
chat-page-failed = The page could not be shown: { $error }
chat-picture = Picture
chat-picture-web = A picture on the web
# $host is the web server the picture comes from.
chat-picture-web-warning = Loading it tells { $host } this machine's address
chat-picture-load = Load from { $host }
chat-visual-drawing = Drawing diagram…
chat-visual-loading = Loading image…
chat-visual-not-drawn = Not drawn: { $error }
chat-visual-image-failed = [image] { $error }
# A code block with no language named.
chat-code = Code
# $lines and $characters are the limits, already formatted.
chat-code-truncated = Preview limited to { $lines } lines and { $characters } characters per line; Copy code includes the full block.
chat-math-unavailable = Equation preview unavailable; original LaTeX is shown below.
chat-summary = Context summary
chat-summary-show = Show
chat-summary-hide = Hide

# The side rail: the agent's plan and the files it changed.
chat-rail-plan = PLAN
chat-rail-changed = CHANGED
chat-rail-files = { $count ->
    [one] { $count } file
   *[other] { $count } files
}
# $done of $total steps of the agent's plan are done.
chat-plan-progress = Plan { $done }/{ $total }

# What still runs after a turn: sub-agents, background shells, and tasks.
chat-running-subagents = { $count ->
    [one] { $count } sub-agent
   *[other] { $count } sub-agents
}
chat-running-shells = { $count ->
    [one] { $count } background shell
   *[other] { $count } background shells
}
chat-running-tasks = { $count ->
    [one] { $count } task
   *[other] { $count } tasks
}
# $summary lists what runs, such as "2 sub-agents · 1 task".
chat-running-still = { $summary } still running
chat-running-what = What
chat-running-time = Time
chat-running-status = Status
chat-running-running = Running
chat-running-done = Done
chat-running-failed = Failed
chat-running-stopped = Stopped

# The agent's settings menu in the chat header.
chat-settings-search = Search…
chat-settings-auto-accept-on = Auto-accepting
chat-settings-auto-accept-off = Asks first
chat-settings-no-match = No match

# Cards and forms an agent's answer shows. $title is the action's own
# label, and $caption what pressing it does.
chat-form-action = { $title } · { $caption }
chat-form-draft = Add to message
chat-form-send = Send to agent
chat-form-read-only = Read-only transcript
# The range a slider takes.
chat-form-range = { $min } to { $max }
chat-form-not-ready = The agent is not ready to receive this form
chat-form-busy = Finish your current message before sending this form
