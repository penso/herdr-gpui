# Settings > Plugins: the values hosts' plugins report, a switch per value,
# and a preview of the sidebar rows they make.

settings-plugins-search-placeholder = Search plugin values...
settings-plugins-config-note = A shown value becomes its own row in [ui.sidebar] of your Herdr config, so the terminal app shows it too.
# What Herdr's endpoint cannot do yet, so the page never implies otherwise.
settings-plugins-limits = Herdr doesn't yet let this app list, enable, or disable plugins or read their logs; use `herdr plugin` on the host. Nothing is installed from here.
settings-plugins-no-window = Open a session window to see what your plugins report. Herdr UI settings remain available.
settings-plugins-read-only = Herdr's settings are read-only on this platform.
settings-plugins-unavailable = Herdr's settings are unavailable. Reload to retry.
settings-plugins-scope-agents = Agents
settings-plugins-scope-spaces = Workspaces
# $scope is Agents or Workspaces, $sample a value a plugin reported, and
# $hosts the hosts reporting it.
settings-plugins-detail = { $scope } · “{ $sample }” · { $hosts }
settings-plugins-detail-missing = { $scope } · not reported
settings-plugins-status-labels = Status labels
settings-plugins-status-labels-detail = { $count ->
        [one] Agents · { $count } names its own status, such as “{ $sample }”
       *[other] Agents · { $count } name their own status, such as “{ $sample }”
    }
settings-plugins-none = No plugin or hook reports custom values on a connected host yet.
settings-plugins-no-match = No matching values.
settings-plugins-native-rows = This app draws its own sidebar rows ([usage] inline = false), so these switches only change the terminal app.
settings-plugins-use-herdr-rows = Use Herdr's sidebar rows in this app
settings-plugins-default-rows = The sidebar uses this app's own rows while Herdr's are the defaults. Showing a value switches it to Herdr's configured rows, in your row style; the preview shows how they look.
settings-plugins-preview = Preview
