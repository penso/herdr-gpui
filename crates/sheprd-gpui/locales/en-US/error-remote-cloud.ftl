# Failures every cloud provider shares: the saved cloud devices, sandbox
# images built from a project's recipe, and the exec stream the app relays a
# sandbox's shell through. $path is a file, $error is the system's text.

error-remote-cloud-catalog = Could not update { $path }: { $error }
error-remote-cloud-json = Invalid saved cloud devices.
# $path is the saved cloud devices file's name.
error-remote-cloud-invalid-catalog = { $path } is not valid.
error-remote-cloud-invalid-device = The cloud device is not valid.
error-remote-cloud-invalid-target = The cloud target is not valid.
error-remote-cloud-missing = The cloud device is no longer saved.
error-remote-cloud-state-directory = No state directory for saved cloud devices.
# $worker selects the background worker that stopped.
error-remote-cloud-worker = { $worker ->
    [destroy] The cloud destroy worker stopped.
    [image-build] The cloud image build worker stopped.
    [image-reset] The cloud image reset worker stopped.
    [job] The cloud job worker stopped.
    [settings] The cloud settings worker stopped.
   *[setup] The cloud setup worker stopped.
    [sign-in] The cloud sign-in worker stopped.
}
error-remote-cloud-process = Could not run the command that reaches the machine.
error-remote-cloud-cancelled = Cancelled.
# $detail is the tail of what the installer or the probe printed on the
# machine.
error-remote-cloud-install = Installing Herdr failed: { $detail }
error-remote-cloud-install-missing = Installing Herdr failed: the installer finished but Herdr is not on the expected paths
error-remote-cloud-install-no-curl = Installing Herdr failed: the machine has no curl; add it to its image or install Herdr there manually
error-remote-cloud-install-no-output = Installing Herdr failed: the installer did not finish
error-remote-cloud-install-unreachable = Installing Herdr failed: the remote command could not reach the machine
# Herdr is missing and installing it was declined. $machine is the cloud
# machine's name; $retry says how the provider's setup offers to install
# it. "Install Herdr" and "Finish setup" are the setup form's controls.
error-remote-cloud-install-not-installed = Installing Herdr failed: Herdr is not installed on { $machine }; { $retry ->
        [add] add it and try again
        [allow] allow installing it and try again
        [finish-setup] enable Install Herdr and use Finish setup
       *[enable] enable Install Herdr and retry
    }
error-remote-cloud-install-not-installed-unnamed = Installing Herdr failed: Herdr is not installed on the machine; { $retry ->
        [add] add it and try again
        [allow] allow installing it and try again
        [finish-setup] enable Install Herdr and use Finish setup
       *[enable] enable Install Herdr and retry
    }
error-remote-cloud-install-timeout = Installing Herdr did not finish within 5 minutes.
error-remote-cloud-unavailable = Cloud devices are unavailable on Windows.

error-remote-cloud-image-busy = Another build of this image is running; wait for it to finish.
error-remote-cloud-image-cancelled = Cancelled.
error-remote-cloud-image-catalog = Could not update { $path }: { $error }
# $provider is a cloud provider's name, such as Daytona. COPY and ADD are
# Dockerfile instructions.
error-remote-cloud-image-context = { $provider } cannot copy files into an image yet: remove the recipe's COPY and ADD lines, or use a container.
error-remote-cloud-image-devcontainer = The dev container definition is not valid JSON.
# $path is the sandbox image records file's name.
error-remote-cloud-image-field-catalog = { $path } is not valid.
error-remote-cloud-image-field-context-path = A file path outside the build context is not valid.
error-remote-cloud-image-field-devcontainer-image = The dev container's image is not valid.
error-remote-cloud-image-field-file-name = A file name that is not UTF-8 is not valid.
error-remote-cloud-image-field-projects = Sandbox images are kept for too many projects.
error-remote-cloud-image-field-project-path = A path outside the project is not valid.
error-remote-cloud-image-host = Could not read the project's recipe from its host.
error-remote-cloud-image-json = Invalid sandbox image records.
# $path is a file in the project, such as .devcontainer/Dockerfile.
error-remote-cloud-image-missing = The recipe names { $path }, which the project does not have.
error-remote-cloud-image-state-directory = No state directory for sandbox images.
error-remote-cloud-image-too-large = The project's recipe has more files than an image build takes.
error-remote-cloud-image-truncated = The project's recipe changed while it was being read; try again.
error-remote-cloud-image-unknown = This app has no record of that project's image.
error-remote-cloud-image-unsupported-compose = This project's recipe uses a dev container defined by Docker Compose, which sandbox images do not support.
error-remote-cloud-image-unsupported-no-build = This project's recipe uses a dev container with neither an image nor a Dockerfile, which sandbox images do not support.
error-remote-cloud-image-unsupported-provider = This project's recipe uses a provider that does not hold images, which sandbox images do not support.

error-remote-cloud-socket-url = The exec stream's address is not valid.
error-remote-cloud-socket-connect = Could not reach the exec stream.
error-remote-cloud-socket-tls = TLS setup for the exec stream failed.
error-remote-cloud-socket-stream = The exec stream failed.
error-remote-cloud-socket-header = Invalid exec stream header.
error-remote-cloud-socket-header-name = Invalid exec stream header name.
error-remote-cloud-socket-closed = The exec stream closed before the command finished.

# What a cloud provider refused, for every provider. $key is a setting such
# as daytona.api_url or an environment variable, as written. Each $value
# branch is a whole sentence; Azure, Boat, Coder, Daytona, OpenSandbox, and
# Sprites are product names, and execd is a program's name.
error-remote-cloud-value-invalid-key = { $key } is not valid.
error-remote-cloud-value-invalid = { $value ->
   *[account] Account is not valid.
    [azure-access-token] Azure access token is not valid.
    [azure-client-secret] Azure client secret is not valid.
    [azure-device-account] Saved Azure device account is not valid.
    [azure-sandbox-endpoint] Azure sandbox endpoint is not valid.
    [azure-sandbox-id] Azure sandbox ID is not valid.
    [azure-ssh-public-key] Azure SSH public key is not valid.
    [boat-api-key] Boat API key is not valid.
    [boat-auto-stop] boat.auto_stop (never or 1–2592000 seconds) is not valid.
    [boat-pending] Boat pending creation is not valid.
    [boat-pending-path] Boat pending path is not valid.
    [boat-pending-size] Boat pending creation size is not valid.
    [boat-sandbox-id] Boat sandbox ID is not valid.
    [boat-ssh-private-key] Boat SSH private key is not valid.
    [boat-ssh-public-key] Boat SSH public key is not valid.
    [coder-identifier] Coder identifier is not valid.
    [coder-workspace] Coder workspace is not valid.
    [container-engine] Container engine is not valid.
    [daytona-api-key] Daytona API key is not valid.
    [daytona-sandbox-id] Daytona sandbox ID is not valid.
    [daytona-snapshot] Daytona snapshot name is not valid.
    [exec-session-id] Exec session ID is not valid.
    [execd-endpoint] Execd endpoint is not valid.
    [opensandbox-api-key] OpenSandbox API key is not valid.
    [remote-command] Remote command is not valid.
    [sandbox-id] Sandbox ID is not valid.
    [sprite-name] Sprite name is not valid.
    [sprite-prefix] Sprite prefix is not valid.
    [sprites-api-token] Sprites API token is not valid.
    [workspace-name] Workspace name is not valid.
}
error-remote-cloud-value-encoding-key = { $key } must be UTF-8.
error-remote-cloud-value-encoding = { $value ->
   *[account] Account must be UTF-8.
    [azure-access-token] Azure access token must be UTF-8.
    [azure-client-secret] Azure client secret must be UTF-8.
    [azure-device-account] Saved Azure device account must be UTF-8.
    [azure-sandbox-endpoint] Azure sandbox endpoint must be UTF-8.
    [azure-sandbox-id] Azure sandbox ID must be UTF-8.
    [azure-ssh-public-key] Azure SSH public key must be UTF-8.
    [boat-api-key] Boat API key must be UTF-8.
    [boat-auto-stop] boat.auto_stop (never or 1–2592000 seconds) must be UTF-8.
    [boat-pending] Boat pending creation must be UTF-8.
    [boat-pending-path] Boat pending path must be UTF-8.
    [boat-pending-size] Boat pending creation size must be UTF-8.
    [boat-sandbox-id] Boat sandbox ID must be UTF-8.
    [boat-ssh-private-key] Boat SSH private key must be UTF-8.
    [boat-ssh-public-key] Boat SSH public key must be UTF-8.
    [coder-identifier] Coder identifier must be UTF-8.
    [coder-workspace] Coder workspace must be UTF-8.
    [container-engine] Container engine must be UTF-8.
    [daytona-api-key] Daytona API key must be UTF-8.
    [daytona-sandbox-id] Daytona sandbox ID must be UTF-8.
    [daytona-snapshot] Daytona snapshot name must be UTF-8.
    [exec-session-id] Exec session ID must be UTF-8.
    [execd-endpoint] Execd endpoint must be UTF-8.
    [opensandbox-api-key] OpenSandbox API key must be UTF-8.
    [remote-command] Remote command must be UTF-8.
    [sandbox-id] Sandbox ID must be UTF-8.
    [sprite-name] Sprite name must be UTF-8.
    [sprite-prefix] Sprite prefix must be UTF-8.
    [sprites-api-token] Sprites API token must be UTF-8.
    [workspace-name] Workspace name must be UTF-8.
}
