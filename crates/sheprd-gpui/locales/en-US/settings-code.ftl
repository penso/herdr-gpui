# Settings > Code: where VS Code tabs' server comes from, its license, the
# check of a server's address, and px0's workbench address.

settings-code-vscode = VS Code
settings-code-start = Start VS Code automatically
settings-code-address = Use an address
settings-code-no-pages = This build shows no pages, so it starts no VS Code.
settings-code-finding = Looking for VS Code…
settings-code-found = VS Code: { $path }
settings-code-missing = VS Code was not found on your PATH or in Applications. Install it to have the app start it, or use the address of a server you run.

settings-code-server = Server
settings-code-start-note = The app runs `code serve-web` on 127.0.0.1 when a VS Code tab first needs it, and stops it when it quits.
settings-code-idle = Not started yet.
settings-code-starting = Starting VS Code… (the first run downloads it)
settings-code-running = Running on { $address }
settings-code-failed = { $error } Retrying…
settings-code-license-terms = VS Code's server is Microsoft software. Starting it accepts the Visual Studio Code Server License Terms and the Microsoft Privacy Statement, so the app starts it only once you accept them.
settings-code-license-link = License Terms
settings-code-privacy-link = Privacy Statement
settings-code-accept = Accept and start

settings-code-address-note = Start a server with `code serve-web`, then paste the address it prints, with its ?tkn= token.
settings-code-address-invalid = Enter an http or https address, such as { $example }
settings-code-test = Test connection
settings-code-testing = Testing…
settings-code-connected = Connected to VS Code
settings-code-commit = commit { $commit }

settings-code-px0 = px0
settings-code-px0-placeholder = Automatic, or { $example }
settings-code-px0-note = Leave the address empty to discover a local workbench or start installed px0 when a tab opens. Set an address to attach to a workbench you run yourself.
settings-code-px0-invalid = Use a loopback HTTP workbench address without credentials, query or fragment.
settings-code-px0-apply = Apply px0 address
