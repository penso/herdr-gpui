# Settings > Cloud devices > Container. Engine names (Docker, Podman), their
# versions, and container names stay as they are.

settings-cloud-container-note = Make containers on this computer's Docker or Podman engine and use them as devices. Each has its own home volume, CPU, memory, and process limits, and runs agents as an unprivileged user without access to this computer's files. Containers share this computer's kernel: for code you do not trust, use a VM provider instead.
settings-cloud-container-checking = Checking for Docker or Podman…
settings-cloud-container-engine-running = { $engine } { $version } is running.

## The engine's fields, each a label and, when not a default value, its placeholder.

settings-cloud-container-engine = Engine
# "docker" and "podman" are what the field takes; keep them as they are.
settings-cloud-container-engine-placeholder = docker or podman; the first one installed if empty
settings-cloud-container-image = Image
settings-cloud-container-cpus = CPUs
settings-cloud-container-memory = Memory
settings-cloud-container-auto-stop = Auto-stop
# "never" and the times are what the field takes; keep them as they are.
settings-cloud-container-auto-stop-placeholder = never; or 30m, 2h: stop once no agent has worked that long

## New container and Containers.

settings-cloud-container-new = New container
settings-cloud-container-new-note = Creates a container with a generated name, pulling the image first if needed, and adds it as a device once Herdr runs in it.
settings-cloud-container-install = Install Herdr in it
settings-cloud-container-create = Create container
settings-cloud-container-no-window = Open a main Herdr window to create a container.
settings-cloud-container-devices = Containers
settings-cloud-container-devices-note = Remove forgets a container and leaves it on the engine. To delete one with its files, use Remove device… in its sidebar menu, which first checks it for unpushed work.
settings-cloud-container-stop = Stop
# A container's row: the engine it runs on, then its state.
settings-cloud-container-running = { $engine } · running
settings-cloud-container-stopped = { $engine } · stopped
settings-cloud-container-missing = { $engine } · missing from the engine
settings-cloud-container-unreachable = { $engine } · engine not reachable
settings-cloud-container-unsaved = Not saved here
settings-cloud-container-unsaved-note = Herdr UI made these containers but has no device for them, such as after quitting while one was made. Add one to use it, or delete it afterwards from its sidebar menu.
settings-cloud-container-adopt = Add
