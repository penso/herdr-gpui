# Disconnecting a paired phone: the question, and the status line's error
# when it fails. $device is the phone's name.

remote-revoke-title = Revoke { $device }?
remote-revoke-detail = It loses remote control of your chats at once: its sessions end and it gets no more notifications. Pairing it again needs a new code.
remote-revoke-hosts = It is also revoked on the machines it was paired with from here, as soon as each can be reached.
remote-revoke-cancel = Cancel
remote-revoke-submit = Revoke
# $error says why.
remote-revoke-failed = Could not revoke { $device }: { $error }
