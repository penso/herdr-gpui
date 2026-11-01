# Notes the user wrote for an agent, on a page in a browser tab, on code
# in the code view, or on terminal text: the Send button, the line under
# notes that wait, and the flash once a batch went somewhere.
# "browser feedback" is a command name and stays as written.

# The Send button: every note was sent, none was, or $count are new.
agent-notes-resend-all = Resend all
agent-notes-send = Send to agent
agent-notes-send-new = Send { $count } new
# The line under notes that wait.
agent-notes-queued-until-idle = Queued until the agent is idle
agent-notes-kept-for-feedback = Kept for `browser feedback`

## Where a batch went

agent-notes-copied-no-agent = No agent to send to, so the notes were copied
agent-notes-sent-waiting = Notes sent to the waiting agent
agent-notes-will-send = Notes will go to the agent once it is idle
agent-notes-sent = Notes sent to the agent
agent-notes-session-not-shown = The agent's session is not shown here; notes copied
agent-notes-daemon-not-selected = The original daemon is not selected; pending notes copied
# Notes no agent took, kept for the agent to fetch: $reason says why.
agent-notes-kept =
    { $reason ->
        [no-agent] No agent runs in that pane; notes kept for `browser feedback`
        [pane-gone] The agent's pane is not here; notes kept for `browser feedback`
        [not-ready] The agent is not ready; notes kept for `browser feedback`
       *[unreachable] Could not reach the agent; notes kept for `browser feedback`
    }
