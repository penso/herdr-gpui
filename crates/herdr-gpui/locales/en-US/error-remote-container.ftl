# Local containers run by Docker or Podman as cloud devices. $engine is the
# program, docker or podman. $name is a container's or volume's name.
# [container] is the config table; engine and max_containers are its keys.

error-remote-container-no-engine = Neither docker nor podman was found. Install one, or set [container] engine.
error-remote-container-spawn = Could not run { $engine }.
# $action is the engine's command that failed, such as pull or inspect;
# $detail is the end of the engine's own error output.
error-remote-container-failed = { $engine } { $action } failed: { $detail }
error-remote-container-timeout = { $engine } { $action } did not finish in time.
error-remote-container-gone = The container { $name } no longer exists.
error-remote-container-suspended = The container { $name } is stopped. Reconnect to start it again.
error-remote-container-name-taken = A container or volume named { $name } already exists; try again for another name.
error-remote-container-not-ours = { $name } on the engine was not made by Herdr UI for this device, so it was left alone.
# $count is the [container] max_containers limit.
error-remote-container-limit = { $count ->
   *[other] There are already { $count } containers, the most [container] max_containers allows. Remove one first.
}
error-remote-container-cancelled = Cancelled.
error-remote-container-build-context = Could not write the image's build context.
# $image is an image's name on the engine.
error-remote-container-image-not-ours = The image { $image } was not built by Herdr UI, so it was left alone.
