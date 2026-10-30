# App Updates: checking GitHub for a new release, downloading, installing,
# and showing its release notes. Versions and the notes are data.

update-panel-title = App Updates
update-panel-subtitle = Updates from GitHub
update-panel-close = Close
update-panel-current-version = Current version
update-panel-latest-version = Latest version
update-panel-not-known = Not yet known
update-panel-whats-new = What's new
# $reason says why, in the updater's own words.
update-panel-disabled = In-app updates unavailable: { $reason }
update-panel-idle = Check GitHub for a new app release.
update-panel-checking = Checking for updates...
update-panel-current = You are running the latest available release.
update-panel-available = A new app release is available.
update-panel-verifying = Download complete. Verifying the update...
update-panel-downloading = Downloading the update
update-panel-downloading-version = Downloading { $version }
update-panel-ready-windows = Download and publisher signature verified. Install when you are ready: Herdr UI quits, its installer runs, and it reopens; your daemon and terminal sessions stay running.
update-panel-ready = Download verified. Install when you are ready; system packages may ask for administrator authentication.
update-panel-installing = Preparing installation and restart. Herdr will quit when the update is ready.
update-panel-homebrew = A new app release is available. Homebrew will install it, refreshing its package metadata if needed.
update-panel-restart-needed = Installed { $version }. Restart to finish; your daemon and terminal sessions stay running.
update-panel-restarting = Starting the updated app...
update-panel-cancelling = Cancelling update... Waiting for current I/O to finish or time out.
update-panel-failed = Update failed: { $error }
# While a system package or Homebrew installs the update.
update-panel-step-approve = Approve the system authentication prompt to install the package...
update-panel-step-asking = Asking Homebrew to upgrade the cask...
update-panel-step-checking = Checking the installed Homebrew cask version...
# $line is a line Homebrew printed, shown as it came.
update-panel-step-homebrew = Homebrew: { $line }
update-panel-step-preparing = Preparing installation...
update-panel-step-refreshing = Refreshing Homebrew metadata with brew update...
update-panel-step-retrying = Retrying Homebrew cask upgrade after refresh...
update-panel-step-starting = Starting Homebrew...
# Under the progress bar. $amount is decimal megabytes with one place.
update-panel-megabytes = { $amount } MB
update-panel-downloaded = { $received } downloaded
update-panel-progress = { $received } of { $total }
update-panel-percent = { $percent }%
update-panel-verified-note = Downloads are verified before installation. Your daemon and terminal sessions stay running.
# Shown only in QA's synthetic update preview.
update-panel-preview = QA preview
update-panel-preview-note = Synthetic update state only. No network or installation is performed. Close or Escape dismisses this preview.
update-panel-manual-releases = Manual Releases
update-panel-release-notes = Release Notes
update-panel-retry = Retry
update-panel-check = Check for Updates
update-panel-download = Download
update-panel-install = Install Update
update-panel-upgrade = Update with Homebrew
update-panel-restart = Restart
update-panel-cancel = Cancel
