# Changelog

All notable changes to Quietly will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Native Windows toast notification when an app exceeds its quota during a
  focus session.
- Crash-recovery marker so a force-killed session does not strand the user's
  HKCU proxy at a dead port.
- Graceful shutdown on window close and tray quit.
- Per-app daily quotas.
- Block rules for `app_path` (executable path) and `title` (window title).
- First-run guide.

### Changed

- Proxy mechanism switched from PAC file to legacy HKCU `ProxyServer` string
  for wider compatibility with Chromium-based browsers.
- Block-rule `kind=url` matching now falls back to a host-vs-host comparison
  when the rule pattern describes a whole domain (e.g.
  `https://www.bilibili.com/` now also matches `http://www.bilibili.com/`).

### Removed

- Schedules (scheduled focus sessions). The schedule list and CRUD UI have
  been removed; focus sessions are now started manually from the Timer page.
- Settings page. App-level settings are now exposed inline on each route.

## [0.1.0] - 2026-08-25

Initial pre-release.

[Unreleased]: https://github.com/yhcpi/quietly/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yhcpi/quietly/releases/tag/v0.1.0