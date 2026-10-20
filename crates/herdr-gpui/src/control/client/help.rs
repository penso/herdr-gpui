//! Usage text for every control command.
use crate::cli::control::FAMILIES;

const COMMON: &str = "\
Every command accepts --json, printing one JSON document, and --window ID,
acting in that window (IDs come from `window list`). By default a command
acts in the window showing the caller's workspace (HERDR_WORKSPACE_ID), and
exits 5 when no window shows it; outside Herdr, in the frontmost window.
Exit status: 0 done, 1 refused, 2 invalid arguments, 3 Herdr UI not running,
4 nothing yet (no notes, no event), 5 not found, 6 busy (a dialog is open),
7 timed out, 8 unsupported on this platform or by this app version.";

const BROWSER: &str = "\
herdr-gpui browser open URL|FILE [--workspace ID] [--no-focus] [--beside|--tab]
    Show a web address, or a local HTML file, in a browser tab of the running
    Herdr UI. The tab joins the caller's own workspace (HERDR_WORKSPACE_ID),
    or the frontmost one outside Herdr. Showing the same page again from the
    same pane reuses and reloads its tab. Bare hosts such as localhost:3000
    are accepted. A file is served with the other files in its folder.
    A file opened from a Herdr pane goes in a split next to that terminal,
    which stays in view and keeps the keyboard; a web address takes a tab of
    the group in use. --beside and --tab choose instead.
herdr-gpui browser reload
    Reload the pages this pane opened, for example after editing the file.
herdr-gpui browser feedback [--wait SECONDS]
    Print the notes the user sent about this pane's pages and have not been
    delivered yet. With --wait, wait up to SECONDS (at most 600) for them.
    Notes are otherwise typed into this pane as a prompt once it is idle.
herdr-gpui browser list
    List page tabs with their IDs.
herdr-gpui browser close|back|forward|text TAB
    Close a page tab, go back or forward in it, or print its visible text.
herdr-gpui browser navigate TAB URL
    Load another web address in a page tab.
herdr-gpui browser zoom TAB +|-|reset|PERCENT
    Zoom a page tab in or out a step, back to 100%, or to PERCENT (50-300),
    as the font size keys do while it is focused. The tab keeps its zoom.
herdr-gpui browser screenshot TAB PATH
    Save a PNG of the page (macOS).
herdr-gpui browser skill
    Print the agent skill that explains these commands.
herdr-gpui browser example
    Print an example explainer page built on the kit Herdr UI serves to local
    pages at /.herdr/kit.css and /.herdr/kit.js.
Exit status: 0 done, 1 refused, 2 invalid arguments, 3 Herdr UI not running,
4 no notes (feedback), 5 no such tab or workspace, 6 busy, 7 timed out,
8 unsupported on this platform.";

fn family(name: &str) -> &'static str {
    match name {
        "status" => {
            "herdr-gpui status\n    Whether Herdr UI runs, its version, protocol, windows, and hosts.\n    Exits 3 when it does not run."
        }
        "skill" => "herdr-gpui skill\n    Print the agent skill describing these commands.",
        "app" => {
            "herdr-gpui app focus|hide|quit\n    Bring the app forward, hide it, or quit it. Quitting leaves Herdr\n    and its terminals running."
        }
        "open" => {
            "herdr-gpui open DIR\n    Open the folder as Open Project in the command palette does: focus its\n    workspace, or start one there, and bring the window forward. DIR must\n    be an existing folder. With no Herdr UI running, starts it and waits up\n    to 20 seconds for its window to reach the local Herdr (exit 7 past that)."
        }
        "window" => {
            "herdr-gpui window [list]\n    The app's windows, with IDs for --window.\nherdr-gpui window new [--session NAME]\n    Open another window: of the caller's own Herdr session, of a named\n    local session, or outside Herdr of the frontmost window's host.\nherdr-gpui window focus ID\nherdr-gpui window fullscreen [on|off|toggle]\nherdr-gpui window size [WIDTHxHEIGHT]\n    Print the window's size, or resize it. Windows cannot be moved.\nherdr-gpui window detach|reconnect\n    Detach from, or reconnect to, the window's selected host."
        }
        "ui" => {
            "herdr-gpui ui sidebar [on|off|toggle]\nherdr-gpui ui statusbar [on|off|toggle]\nherdr-gpui ui font-size +|-|reset|POINTS\nherdr-gpui ui panel workspaces|overview|pull-requests|issues|security|usage\n    For this window and session; settings are changed with `config`."
        }
        "tab" => {
            "herdr-gpui tab [list]\n    The app's own tabs: pages, code, reviews, Devices.\n    Terminal tabs belong to Herdr: use `herdr tab`.\nherdr-gpui tab focus|close ID\n    Show a tab, switching to its workspace, or close it."
        }
        "code" => {
            "herdr-gpui code open FILE[:LINE]\n    Show a local file read-only in a code tab, at LINE.\nherdr-gpui code edit FILE[:LINE]\n    Open it in your terminal editor ($VISUAL, $EDITOR) in a new pane."
        }
        "devices" => {
            "herdr-gpui devices [open]\n    Open the Devices overview tab in the caller's workspace."
        }
        "orchestrator" => {
            "herdr-gpui orchestrator [open] [--repo PATH]\n    Show the Issues section narrowed to the caller's project, or PATH's.\nherdr-gpui orchestrator items|issues|prs [--repo PATH]\n    List what it last synced, from its local database (no network).\n    Runs here; Herdr UI need not run. Exits 4 before its first sync."
        }
        "review" => {
            "herdr-gpui review [open]\n    Open the review of the caller's workspace's uncommitted changes."
        }
        "notes" => {
            "herdr-gpui notes [list]\n    Every worktree note, most recently edited first.\nherdr-gpui notes get|clear\nherdr-gpui notes set TEXT...\n    The note on the caller's checkout (its repository and branch), or the\n    focused workspace's outside a Herdr pane. Notes are one line of at most\n    500 characters. `list` and `get` exit 4 when there is none."
        }
        "feedback" => {
            "herdr-gpui feedback [--wait SECONDS]\n    Notes the user sent this pane: page, review, and terminal notes that\n    were not typed into it. Exits 4 when there are none."
        }
        "checkpoint" => {
            "herdr-gpui checkpoint [list]\n    The current directory's checkout's checkpoints, newest first.\nherdr-gpui checkpoint create [LABEL...]\n    Take one now.\nherdr-gpui checkpoint diff CHECKPOINT [CHECKPOINT]\n    What changed between two, or since one in the working tree (untracked\n    files included).\n    These run here, on the current branch; Herdr UI need not run.\nherdr-gpui checkpoint restore [CHECKPOINT]\n    Open the caller's checkout's checkpoints dialog in Herdr UI with\n    CHECKPOINT chosen; the user confirms the restore there."
        }
        "scripts" => {
            "herdr-gpui scripts [list]\n    The current directory's repository's worktree scripts\n    (.herdr/worktree.toml). Runs here; Herdr UI need not run.\nherdr-gpui scripts setup|run\n    Run one in a new tab of the caller's workspace (setup: linked worktrees\n    only). Herdr UI asks before running a script it has not seen. The\n    archive script runs only when a worktree is removed in Herdr UI."
        }
        "teleport" => {
            "herdr-gpui teleport --to HOST\n    Open the teleport dialog for the caller's linked worktree, reviewing\n    HOST (a host ID or label). The user confirms the move there."
        }
        "fan-out" => {
            "herdr-gpui fan-out PROMPT... [--agent KIND[:COUNT]]...\n    Open the fan-out dialog for the caller's repository with PROMPT and\n    these agents (claude, codex, ...; at most 6 in all) chosen once the\n    host's agents are known. The user starts it there."
        }
        "pr" => {
            "herdr-gpui pr [status]|checks\n    The pull request of the caller's workspace's branch, as Herdr UI last\n    looked it up (selected host only; sign in to GitHub first). Exits 4\n    when there is none.\nherdr-gpui pr comments\n    Its review comments (the workspace must be the focused one).\nherdr-gpui pr open\n    Open it in a browser tab."
        }
        "host" => {
            "herdr-gpui host [list]\n    The window's hosts and their status. Herdr's `machine` command\n    manages saved hosts.\nherdr-gpui host select ID\n    Show that host in the window, as choosing it in the sidebar does."
        }
        "cloud" => {
            "herdr-gpui cloud [list]\n    Saved cloud machines (Herdr UI need not run).\nherdr-gpui cloud add\n    Open the settings that add a machine. A machine is never created or\n    destroyed from the command line.\nherdr-gpui cloud setup ID\n    Run the setup script from [cloud] in config-gpui.toml again on a saved\n    machine, named by its ID or label, in a new tab once it is shown."
        }
        "ports" => {
            "herdr-gpui ports [list]\n    Ports the caller's workspace's processes listen on, and forwards.\n    Listening ports are known only while show_listening_ports is on.\nherdr-gpui ports forward|unforward PORT\n    Forward a port of the workspace's SSH host to this machine, or stop.\nherdr-gpui ports open PORT\n    Show localhost:PORT in a browser tab, forwarding it first if remote."
        }
        "remote-files" => {
            "herdr-gpui remote-files [list]\n    Copies of SSH hosts' files that link clicks fetched, with whether\n    each is synced, edited, or in conflict. Uploads are confirmed in the\n    app."
        }
        "processes" => {
            "herdr-gpui processes [--pane ID]\n    The processes under a pane (default HERDR_PANE_ID), with CPU and\n    memory. Runs here; Herdr UI need not run."
        }
        "usage" => "herdr-gpui usage\n    AI plan usage on the window's selected host.",
        "security" => {
            "herdr-gpui security\n    Private vulnerability reports in triage (titles only) and open\n    Dependabot alerts of the github.com repositories the window's\n    workspaces hold, most severe first, as the Security section last read\n    them (sign in to GitHub first). Exits 4 when there are none.\n    `ui panel security` shows the section.\nherdr-gpui security open REPOSITORY ALERT|GHSA\n    Show alert ALERT (its number) or private report GHSA (its GHSA ID) of\n    REPOSITORY (owner/name) in the Security section, beside its list, as\n    the section last read it."
        }
        "system" => {
            "herdr-gpui system\n    The window's selected host's load, when Herdr UI samples it."
        }
        "search" => {
            "herdr-gpui search symbol|file QUERY... [--limit N]\n    Go to Symbol or Go to File over the current directory's checkout,\n    printing paths relative to its root. Runs here; Herdr UI need not run."
        }
        "config" => {
            "herdr-gpui config [path]\n    The settings files.\nherdr-gpui config get KEY\n    The setting's value, as TOML: your override, else the default.\nherdr-gpui config set KEY VALUE\nherdr-gpui config unset KEY\n    Change a setting in config-gpui.local.toml. VALUE is read as TOML\n    (14, true, [\"a\"], { show = true }), else as text. A value or key\n    Herdr UI would not load is refused and the file left as it was.\n    A running Herdr UI applies the change at once. KEY is dotted, such\n    as terminal.size. Unsetting a key that is not set exits 4.\nherdr-gpui config check\n    Report settings that do not load and unknown keys, and, when Herdr UI\n    runs, fonts that are not installed; exits 1 when there are any.\nherdr-gpui config reload\n    Reread Herdr's config and this app's.\nOnly reload needs Herdr UI running."
        }
        "theme" => {
            "herdr-gpui theme [list]\n    The themes there are; * marks the selected one.\nherdr-gpui theme set NAME...\n    Choose and save a theme, or light:NAME,dark:NAME to follow the\n    system appearance. Saved even when Herdr UI is not running."
        }
        "font" => {
            "herdr-gpui font [list]\n    The installed font families.\nherdr-gpui font set NAME...\nherdr-gpui font reset\n    The font of every face; reset goes back to the default.\nThese need Herdr UI running, which knows the installed fonts."
        }
        "keybinds" => {
            "herdr-gpui keybinds [list]\n    The window's shortcuts.\nherdr-gpui keybinds check\n    Herdr's custom commands this client cannot reach; exits 1 if any."
        }
        "caffeine" => {
            "herdr-gpui caffeine [status]\nherdr-gpui caffeine on|off|toggle [--for 90s|30m|2h]\n    Keep the display and the machine awake. With --for (at most 24h),\n    caffeine turns off then, unless it was changed in the meantime."
        }
        "sound" => {
            "herdr-gpui sound [list]\n    The notification sounds.\nherdr-gpui sound play [done|request]\n    Play one now, from the file Herdr's sound settings name or the\n    built-in one."
        }
        "bell" => "herdr-gpui bell\n    Ring the system bell, as a terminal bell does.",
        "events" => {
            "herdr-gpui events [--after CURSOR] [--wait SECONDS] [--kind KIND]... [--follow]\n    What happened since CURSOR, the INSTANCE:SEQ the last call printed:\n    windows and tabs opening and closing, notes, and host status. A cursor\n    from an earlier app run gets everything this run kept. --follow keeps\n    printing until interrupted. Kinds:\n    window_opened, window_closed, window_focused, tab_opened, tab_closed,\n    feedback, note, host."
        }
        "wait" => {
            "herdr-gpui wait feedback|window|online|tab-closed ID [--timeout SECONDS]\n    Wait (default 60, at most 600 seconds). Exits 4 when time runs out."
        }
        "run" => {
            "herdr-gpui run COMMAND\n    Run an app command by its [keybindings] name, as the command palette\n    does, for example toggle_sidebar. See `commands`."
        }
        "commands" => "herdr-gpui commands\n    The commands `run` accepts, with their shortcuts.",
        "logs" => {
            "herdr-gpui logs [--lines N] [--path]\n    The app's latest log records (default 100, at most 5000), or the log file's path.\n    Runs here; Herdr UI need not run. Exits 4 when there is no log yet."
        }
        "update" => {
            "herdr-gpui update [status]\nherdr-gpui update check\n    Ask for the latest release. Never downloads or installs. Local\n    builds report updates as off."
        }
        "diagnostics" => {
            "herdr-gpui diagnostics bundle PATH\n    Write build information, settings diagnostics, your settings\n    overrides, and recent logs to a new file at PATH for a bug report,\n    with home paths shortened and secret-looking values redacted. Read\n    it before sharing. Never replaces an existing file. Runs here."
        }
        "integrations" => {
            "herdr-gpui integrations [list]\n    The agent integrations Herdr offers the window's host. Install them\n    from Herdr UI's integrations settings.\nherdr-gpui integrations install-skill\n    Install or update the agent skill for Claude Code and other agents.\n    Runs here; Herdr UI need not run."
        }
        _ => "",
    }
}

/// Usage for one family, or the list of families.
pub(super) fn usage(name: Option<&str>) -> String {
    match name {
        Some("browser") => BROWSER.to_owned(),
        Some(name) => format!("{}\n\n{COMMON}", family(name)),
        None => {
            let mut text = String::from("herdr-gpui COMMAND ... [--json] [--window ID]\n\n");
            for (name, summary) in FAMILIES {
                text.push_str(&format!("  {name:<14}{summary}\n"));
            }
            text.push_str("\nRun `herdr-gpui COMMAND --help` for one command's usage.\n\n");
            text.push_str(COMMON);
            text
        }
    }
}
