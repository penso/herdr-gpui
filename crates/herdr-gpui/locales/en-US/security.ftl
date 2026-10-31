# The Security section: private vulnerability reports in triage and
# Dependabot alerts across the window's repositories, and an item's page.
# Repository, package, advisory, and reporter names and what GitHub writes
# are shown as they are.

security-title = Security
security-subtitle = Private reports and Dependabot alerts
security-refresh = Refresh
security-updating = Updating…
# When the last read finished: $ago is how long ago, such as 5 min ago.
security-updated = Updated { $ago }
security-no-workspace = Open a workspace first

security-reading-repositories = Reading the repositories…
security-sign-in = Sign in to GitHub in Herdr (account button, top right) to read security alerts and reports.
security-no-repository = No github.com repository is open in this window's workspaces.
security-no-match = Nothing matches the search and the pills picked.
# $repositories lists repository names.
security-quiet = No open alerts this account can see: { $repositories }
security-alerts-off = Dependabot alerts are off: { $repositories }
security-alerts-unreadable = Dependabot alerts are not readable with this GitHub sign-in: { $repositories }
security-reports-unreadable = Private vulnerability reports are not readable with this GitHub sign-in: { $repositories }
# A repository GitHub would not answer for.
security-repository-failed = { $repository }: { $error }
security-reports-failed = private reports: { $error }

security-reported = Reported to you
# $more is "yes" when GitHub has more than the page read.
security-reported-count = { $more ->
        [yes] { $count }+ private vulnerability reports in triage
       *[no] { $count ->
            [one] { $count } private vulnerability report in triage
           *[other] { $count } private vulnerability reports in triage
        }
    }
security-in-triage = { $count } in triage
security-reported-by = reported by { $login }
security-open-count = { $count } open
security-open-of = { $shown } of { $total } open
# A repository's card heading: its host, then how many alerts are open.
security-repository-detail = { $host } · { $count }
security-fixed-in = fixed in { $version }
security-no-fix = no fixed version yet
security-dependabot = Dependabot
security-start-agent = Start agent
security-filter = Filter: summary, package, advisory, repository, reporter…

security-severity = { $severity ->
        [critical] Critical
        [high] High
        [moderate] Moderate
       *[low] Low
    }
security-unrated = Unrated
security-severity-cvss = { $severity ->
        [critical] Critical
        [high] High
        [moderate] Moderate
       *[low] Low
    } · CVSS { $score }
security-reporter-rating = { $severity ->
        [critical] Critical
        [high] High
        [moderate] Moderate
       *[low] Low
    } · reporter's rating

# An item's page.
security-private-reports = Private reports
security-show-list = Show the list
security-hide-list = Hide the list
# Which item of the list is shown.
security-position = { $at } of { $count }
security-previous = Previous
security-next = Next
security-open-on-github = Open on GitHub
security-close = Close
security-tag-open = Open
security-tag-triage = Triage
security-tab-overview = Overview
security-tab-advisory = Advisory
security-tab-dependency = Dependency path
security-tab-report = Report
security-tab-draft = Advisory draft
security-tab-conversation = Conversation
security-not-read = Not read from GitHub.
security-reading = Reading from GitHub…
security-page-failed = GitHub would not give this page: { $error }
security-try-again = Try again
security-no-description = No description.
# $date is a date such as Oct 14, 2026, $ago how long ago it was.
security-date = { $month ->
        [1] Jan
        [2] Feb
        [3] Mar
        [4] Apr
        [5] May
        [6] Jun
        [7] Jul
        [8] Aug
        [9] Sep
        [10] Oct
        [11] Nov
       *[12] Dec
    } { $day }, { $year } · { $ago }

# An alert's page.
security-what-happened = What happened
security-upgrade-to-fix = Upgrade to fix
security-no-fixed-version = No fixed version yet
security-copied = Copied the command
security-pulls-in = { $manifest } pulls in an affected version.
security-no-fix-known = GitHub knows no fixed version: remove the dependency or mitigate it.
security-fix-with-agent = Fix with agent
# $file is a manifest's file name, such as Cargo.lock.
security-open-file = Open { $file }
security-dismiss = Dismiss on GitHub
security-direct = Direct: the manifest names it
security-transitive = Transitive: another dependency pulls it in
security-runtime = Runtime: it ships with the project
security-development = Development: it only builds or tests the project
security-unknown = GitHub could not tell
security-dependabot-fix = Dependabot's fix
security-lockfile-note = GitHub does not say which dependency pulls it in; the lockfile does.
security-open-lockfile = Open the lockfile
security-references = References
security-fact-package = Package
security-fact-relationship = Relationship
security-fact-scope = Scope
security-fact-manifest = Manifest
security-fact-requirement = Requirement
security-fact-affected-versions = Affected versions
security-fact-severity = Severity
security-fact-installed = Installed
security-fact-identifiers = Identifiers
security-fact-weakness = Weakness
security-fact-opened = Opened
security-fact-score = Score
security-fact-vector = Vector
security-fact-published = Published
security-fact-rating = Rating
security-fact-affected = Affected
security-fact-draft = Draft advisory
security-fact-reporter = Reporter
security-fact-filed = Filed
security-fact-credit = Credit

# A report's page.
security-report = Report
security-investigate = Investigate with agent
security-triage = Accept or close on GitHub
security-private = Private until published
security-private-note = Only the repository's maintainers and the reporter see it. Accepting opens the draft advisory; the reporter's thread stays on GitHub.
# A draft's package: $package, its ecosystem, the affected versions, and
# whether a fixed version is published.
security-draft-package = { $package } ({ $ecosystem }) { $vulnerable } · { $fixed ->
        [yes] fixed in { $patched }
       *[no] no fixed version yet
    }
security-draft-empty = The draft has no packages, score, or credits yet.
security-no-comments = No comments yet.
security-reply = Reply on GitHub
security-read-on-github = Read on GitHub
security-comments-unavailable = GitHub would not list this report's comments: its comments API is a preview for public repositories.
security-comments-more = Only the first { $count } comments are shown.

# Starting an agent on an item.
security-no-host = Show a host with a workspace on { $repository } to start agents there
security-menu-closed = The workspace's menu did not open
security-agents-need-git = Agents need a Git workspace on this machine or an SSH host
security-clear-search = Clear search
