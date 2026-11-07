# Why the px0 workbench could not be found, started, or reached.
# px0 is a program's name; px0.url is a config key.

error-px0-address = Use a loopback HTTP px0 workbench address, without credentials, query or fragment
error-px0-missing = px0 is not installed; install it on your login shell's PATH, or set px0.url
error-px0-state-directory = Cannot locate the app's state directory
error-px0-discovery = Cannot read px0 discovery information: { $error }
error-px0-spawn = Cannot start px0: { $error }
error-px0-worker = Cannot start the px0 worker: { $error }
error-px0-http = Cannot reach the px0 workbench: { $error }
error-px0-read = Cannot read the px0 response: { $error }
error-px0-json = Invalid px0 workbench response: { $error }
error-px0-not-workbench = This address did not answer as a px0 workbench
error-px0-timeout = px0 did not publish its workbench address in time
error-px0-exited = px0 stopped before its workbench was ready
error-px0-process = Cannot check the px0 process: { $error }
error-px0-shutdown = The previous px0 worker did not finish stopping: { $error }
