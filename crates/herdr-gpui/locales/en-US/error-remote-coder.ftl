# Coder workspaces as cloud devices. $detail is Coder's own text, kept as
# it is. [coder] is the config table; url and cli are its keys.

# $field names a setting, such as coder.url.
error-remote-coder-url = { $field } must be https (http only on loopback) with no credentials, query, or fragment.
# $key is the missing [coder] setting, such as url or oauth_client_id.
error-remote-coder-missing = Coder needs { $key }. Set it in Settings > Cloud devices.
error-remote-coder-authentication = Coder sign-in required.
error-remote-coder-client = Coder rejected the OAuth client secret. Copy the OAuth2 app's secret from Coder into Settings > Cloud devices and sign in again.
error-remote-coder-forbidden = Coder denied access. Check your account's permissions for this template.
# $code is the HTTP status code, such as 500.
error-remote-coder-status = Coder request failed (HTTP { $code })
error-remote-coder-status-message = Coder request failed (HTTP { $code }): { $detail }
error-remote-coder-network = Coder network request failed or timed out.
error-remote-coder-read = Coder response could not be read within the size/time limit.
error-remote-coder-size = Coder response exceeded the size limit.
error-remote-coder-json = Invalid Coder JSON response.
error-remote-coder-header = Invalid Coder authorization header.
# $address is the local address the sign-in waits on, such as 127.0.0.1:8085.
error-remote-coder-listen = Could not listen for the Coder sign-in redirect on { $address }.
error-remote-coder-redirect = The Coder sign-in redirect could not be read.
error-remote-coder-cancelled = Coder sign-in was cancelled.
error-remote-coder-timeout = Coder sign-in timed out. Sign in again.
# $detail is the OAuth error Coder returned, such as access_denied.
error-remote-coder-authorization = Coder did not authorize this sign-in ({ $detail }).
error-remote-coder-token = Invalid Coder token response.
error-remote-coder-random = Could not generate a random sign-in secret.
error-remote-coder-workspace = Coder workspace failed: { $detail }
error-remote-coder-deleted = The Coder workspace was deleted.
error-remote-coder-build-timeout = The Coder workspace did not become ready within 10 minutes.
error-remote-coder-delete-timeout = The Coder workspace was not deleted within 10 minutes. Check it in Coder.
error-remote-coder-cli = The coder CLI was not found. Install it or set [coder] cli to its path.
error-remote-coder-deployment = This device belongs to a Coder deployment that is no longer configured in [coder] url.
# $path is a file; $error is the system's text.
error-remote-coder-catalog = Could not update { $path }: { $error }
# $worker selects the background worker that stopped. Coder is a product
# name.
error-remote-coder-worker = { $worker ->
    [destroy] Coder destroy worker stopped.
    [image-build] Coder image build worker stopped.
    [image-reset] Coder image reset worker stopped.
    [job] Coder job worker stopped.
    [settings] Coder settings worker stopped.
   *[setup] Coder setup worker stopped.
    [sign-in] Coder sign-in worker stopped.
}
# $name is a template's display name; $matches lists the templates shown so.
error-remote-coder-ambiguous-template = Several Coder templates are shown as { $name } ({ $matches }); name one by its name or ID.
# $available lists the deployment's templates by name.
error-remote-coder-template = Coder has no template named { $name }; it has { $available }.
# --template is a command line option.
error-remote-coder-template-unnamed = Coder has several templates ({ $available }); name one with --template.
# $template is a template's name; $available lists its presets.
error-remote-coder-preset = The Coder template { $template } has no preset named { $name }; it has { $available }.
