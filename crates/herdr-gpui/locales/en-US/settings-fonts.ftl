# Settings > Fonts: the faces, the font picker, and the specimen. Each
# face's title is in settings.ftl; font family names are data.

settings-fonts-section-faces = Faces
settings-fonts-use-one = Use one family for all…
# The family `.SystemUIFont`, the platform's interface font.
settings-fonts-system-font = System font
settings-fonts-size = { $size } px

## The specimen of the face last picked, set in that face.

settings-fonts-specimen-terminal = Specimen · Terminal
settings-fonts-specimen-sidebar = Specimen · Sidebar
settings-fonts-specimen-tabs = Specimen · Tabs
settings-fonts-specimen-ui = Specimen · Interface
# Sample text shaped like the sidebar, a line each: a workspace, then two
# agents and their status.
settings-fonts-sample-sidebar-1 = herdr-gpui / workspace
settings-fonts-sample-sidebar-2 = {"  "}claude  Working
settings-fonts-sample-sidebar-3 = {"  "}codex   Ready for review
# Sample text shaped like two tab strips, a line each.
settings-fonts-sample-tabs-1 = main.rs   agent / review
settings-fonts-sample-tabs-2 = Changes   Terminal   Preview
settings-fonts-sample-ui-1 = Make room for focused work.
settings-fonts-sample-ui-2 = Your preferences, your workspace.

## The font picker. $role is which face it sets: all, terminal, sidebar,
## tabs, or ui.

settings-fonts-picker-title =
    Font family / { $role ->
        [all] All roles
        [terminal] Terminal
        [sidebar] Sidebar
        [tabs] Tabs
       *[ui] Interface
    }
settings-fonts-picker-default =
    Use platform default family / { $role ->
        [all] All roles
        [terminal] Terminal
        [sidebar] Sidebar
        [tabs] Tabs
       *[ui] Interface
    }
settings-fonts-picker-close = Close
settings-fonts-loading = Loading installed fonts...
settings-fonts-count =
    { $total ->
        [one] { $shown } of { $total } installed family
       *[other] { $shown } of { $total } installed families
    }
settings-fonts-no-match = No matching fonts.
settings-fonts-search-placeholder = Search installed fonts...
