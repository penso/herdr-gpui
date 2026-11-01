# ZoomMate's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-zoommate-setting-cookie = Your Zoom session cookies. Sign in at https://zoommate.zoom.us, open Developer Tools > Application > Cookies for https://zoommate.zoom.us (Zoom's sign-in cookies such as _zm_ssid, _zm_lang and cf_clearance live on zoom.us), and paste them all as "name=value; name2=value2". They are exchanged for a short-lived token on every refresh.
usage-zoommate-setting-token = A bearer token instead of cookies: on https://zoommate.zoom.us open Developer Tools > Network, reload the AI credit usage page, select the credits/status request and copy its Authorization header without "Bearer ". It expires after about an hour.
