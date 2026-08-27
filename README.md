# Quietly

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/yhcpi/quietly/actions/workflows/ci.yml/badge.svg)](https://github.com/yhcpi/quietly/actions/workflows/ci.yml)

A Windows desktop focus / productivity app. Pick a duration, hit Start,
and Quietly routes distracting sites / apps through a local proxy that
returns a block page for everything you listed — without a global VPN,
without DNS surgery, without admin rights.

Built on Tauri 2 (Rust + WebView2) + React + TypeScript.

See [CONTRIBUTING.md](CONTRIBUTING.md) to hack on it, [SECURITY.md](SECURITY.md)
to report a vulnerability, and [CHANGELOG.md](CHANGELOG.md) for release notes.
By participating, you agree to [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## What it does

- **Block rules** by domain (`*.reddit.com`), URL (`https://twitter.com/home`),
  app path (`C:\...\Discord.exe`), or window title.
- **Focus sessions** with optional auto-block, lock (no cancel mid-session),
  per-app daily quotas, whitelist precedence.
- **Usage tracking** for foreground apps and per-host network traffic,
  surfaced on the Stats page.
- **Native Windows toast** when an app exceeds its quota during a session.
- **Tray icon** — closing the X hides to tray; the tray Quit menu item
  is the only path that actually exits the process (so a focus session
  isn't killed by an accidental click on the X).

## System requirements

- Windows 10 1809+ or Windows 11
- Microsoft Edge WebView2 Runtime (preinstalled with Edge on most systems;
  if not, see `scripts/README.txt` inside the portable ZIP for the install link)

## Build from source

Requires: Node 18+, pnpm, Rust 1.78+ with the `rustfmt` and `clippy`
components, and the Microsoft C++ Build Tools (`cargo-msvc.bat` in
`scripts/` shows the env setup this repo was last built on).

```bash
pnpm install
pnpm tauri build --no-bundle    # release binary at src-tauri/target/release/quietly.exe
scripts\_package_only.bat       # portable ZIP at dist-portable/quietly-portable-YYYYMMDD.zip
```

Run tests:
```bash
pnpm vitest run                       # frontend
cd src-tauri && cargo test --tests    # Rust
```

## Where your data lives

Everything Quietly writes at runtime goes under `%APPDATA%\com.quietly.app\`:

```
quietly.db            SQLite (rules, sessions, hits, quotas, usage log)
proxy.pac             PAC file (always a valid JS function returning DIRECT)
proxy-marker.json     snapshot of your pre-existing proxy settings, for crash recovery
first_run_acked       sentinel file — presence means the first-run guide is dismissed
proxy-debug.log       request log (when log level is verbose)
```

To reset to a clean state, delete that folder and relaunch.

## How the proxy works

Quietly sets `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings\ProxyEnable=1`
and `ProxyServer=127.0.0.1:<port>` while a focus session is running.
`<port>` is kernel-assigned at session start (no fixed-port collision risk
on a fresh machine), and the original registry values are snapshotted
into `proxy-marker.json` so the next session restores them.

On clean exit, the snapshot is restored. On crash, the snapshot is
restored on the next launch via `proxy_marker::reap` in `lib.rs`.

### Browser compatibility

| Browser | Auto-block works? |
|---|---|
| Microsoft Edge | yes (WinINET) |
| Google Chrome | yes (WinINET) |
| Opera, 360 Browser, QQ Browser | yes (WinINET) |
| Internet Explorer | yes (WinINET) |
| **Firefox** | **no** — uses its own proxy settings, ignores HKCU |
| **Tor Browser** | **no** — same as Firefox |

For Firefox / Tor, block by `app_path` or `title` rule instead.

### Known limitation — HTTPS block pages

Blocking an HTTPS site shows the browser's generic
`ERR_TUNNEL_CONNECTION_FAILED` page rather than a friendly "this site is
blocked" message. This is a Chromium protocol limitation: CONNECT
tunnels only get an HTTP 4xx from the proxy, and the browser surfaces
it as a connection error before any HTTP body can be shown. Displaying
a custom page for HTTPS would require TLS interception (installing a
trusted root CA on the host), which is out of scope. **The block itself
still works — the connection fails and the site cannot load.**

## Development helpers

`scripts/` contains:

- `build-portable.bat` — full release build + ZIP
- `_package_only.bat` — re-zip an already-built release binary
- `README.txt` — the user-facing README that goes inside the portable ZIP
- `cargo-msvc.bat` — invoke `cargo` with the right VS BuildTools env
- `check-proxy-now.bat` — print current HKCU proxy state + marker file
- `reset-reg.bat` — emergency "I can't browse anything" reset
- `verify-rules.bat` — print DB rules + proxy state + recent log

## Project layout

```
src/                    React frontend
  components/           shared UI (Layout, FirstRunGuide, …)
  routes/               one file per page (Timer, Rules, Stats, …)
  lib/                  pure helpers (rule matching, stats, formatting)
  ipc.ts                thin wrapper around Tauri commands
src-tauri/              Rust backend
  src/engine/           timer, proxy, PAC, guard, app usage
  src/ipc/              Tauri command handlers
  src/storage/          SQLite + filesystem-backed persistence
  src/lib.rs            Tauri setup, WebView2 detection, crash recovery
  migrations/           SQL schema (single 0001_init.sql)
  assets/block.html     the page returned for blocked HTTP requests
  tests/                integration tests
  capabilities/         Tauri 2 capability definitions
```

## License

MIT — see [LICENSE](LICENSE).
