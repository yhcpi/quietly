Quietly - Portable Edition
==========================

Run quietly.exe to start. No installation required.

System requirements:
  - Windows 10 1809+ or Windows 11
  - Microsoft Edge WebView2 Runtime (preinstalled with Edge on most systems)

If the app fails to start with a "webview" error, install the
Evergreen Runtime from:
  https://developer.microsoft.com/microsoft-edge/webview2/

Data is stored in %APPDATA%\com.quietly.app\

To uninstall, simply delete this folder and the %APPDATA% data dir.

Known limitation - HTTPS block pages:
  Blocking an HTTPS site shows the browser's generic
  "ERR_TUNNEL_CONNECTION_FAILED" page rather than a friendly
  "this site is blocked" message. This is a Chromium protocol
  limitation: CONNECT tunnels get only an HTTP 4xx from the
  proxy, and the browser surfaces it as a connection error
  before any HTTP body can be shown. Displaying a custom
  page for HTTPS would require TLS interception (installing a
  trusted root CA on the host), which is out of scope for
  this build. The block itself still works - the connection
  fails and the site cannot load.

Browser compatibility:
  Auto-block routes WinINET traffic (Edge, Chrome, Opera, IE,
  360 Browser, QQ Browser) through Quietly's local proxy.
  Firefox and Tor Browser do NOT honor HKCU proxy settings —
  if you need to block those, use the per-app guard rule list.
