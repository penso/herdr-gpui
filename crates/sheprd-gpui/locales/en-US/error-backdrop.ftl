# The window's background picture. backdrop.opacity and backdrop.image are
# config keys. $path is the picture's file; $error is the system's or the
# image decoder's text.

error-backdrop-decode = Could not decode the backdrop image { $path }: { $error }
error-backdrop-empty-image = backdrop.image must not be empty
error-backdrop-image-path-encoding = backdrop.image must be a UTF-8 path
error-backdrop-not-a-file = The backdrop image { $path } is not a regular file
error-backdrop-opacity = backdrop.opacity must be finite and between 0 and 1
error-backdrop-read = Could not read the backdrop image { $path }: { $error }
error-backdrop-relative-image = backdrop.image must be an absolute path or start with ~/
# $limit is a size in bytes.
error-backdrop-too-large = The backdrop image { $path } exceeds the { $limit }-byte limit
