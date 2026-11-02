# Failures reading or writing the daemon's wire protocol: a frame is one
# message on the connection, a surface is a pane's screen contents, and a
# patch or delta updates one. $error is the decoder's own diagnostic.

error-protocol-cell-count = cell count does not match frame dimensions
error-protocol-clipboard-image-extension = unsupported clipboard image extension
error-protocol-clipboard-image-size = clipboard image must contain 1 byte through 16 MiB
error-protocol-clipboard-image-target = clipboard image target must contain 1 through 1024 bytes
error-protocol-cursor-bounds = cursor outside frame
error-protocol-decode = frame decoding failed: { $error }
error-protocol-delta-grid-limit = surface delta grid exceeds its limits
error-protocol-delta-metadata-cells = surface delta metadata carries cells
error-protocol-delta-popup = surface delta popup update does not match its popup
error-protocol-delta-span = surface delta span is outside its grid, unsorted, or over budget
error-protocol-empty-frame = empty frame
error-protocol-encode = frame encoding failed: { $error }
error-protocol-encoding-json = invalid surface encoding JSON: { $error }
error-protocol-encoding-limit = surface encoding exceeds the frame limit
error-protocol-encoding-payload = invalid surface encoding payload: { $error }
error-protocol-frame-limit = frame exceeds limit
error-protocol-hyperlink-index = invalid hyperlink index
error-protocol-io = protocol I/O failed: { $error }
error-protocol-patch-cursor-bounds = patch cursor outside frame
error-protocol-patch-geometry = surface patch changed pane geometry
error-protocol-patch-identity = surface patch identity mismatch
error-protocol-patch-row-bounds = surface patch row outside frame
error-protocol-scroll-bounds = surface scroll region is outside the frame or overlaps another
error-protocol-scroll-count = surface scroll has an invalid scroll count
error-protocol-scroll-payload = surface scroll does not carry a pane patch
error-protocol-scroll-truncated = surface scroll is truncated
error-protocol-surface-baseline = surface update does not match its baseline
error-protocol-trailing-bytes = trailing frame bytes
