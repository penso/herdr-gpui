; The installer's own English text, on top of Inno Setup's compiler:Default.isl.
; A translation is a copy of this file named for the app's language tag
; (installer/fr.isl), listed in herdr-gpui.iss's [Languages] after the
; matching compiler:Languages\*.isl (docs/i18n.md). %1 is the argument the
; script passes; keep every name, and keep "herdr-gpui" and file paths as
; they are.

[CustomMessages]
AddToPath=Add herdr-gpui to PATH, for terminals and agents
; The protocol's name in Windows' default apps list; %1 is the app's name.
LinkName=%1 link
; The folder context menu's command; %1 is the app's name.
OpenIn=Open in %1
; %1 is the path of herdr-automate.exe.
AutomateDidNotStop=The automation daemon at %1 did not stop. Stop its runs with herdr-automate stop, then run Setup again.
AutomateStillRunning=The automation daemon at %1 is still running. Quit it, then run Setup again.
