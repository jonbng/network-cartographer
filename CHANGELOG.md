# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-07-24

### Added

- Desktop app (Tauri 2) with live per-app TCP connection monitoring
- Automatic traceroute queue with per-OS probe methods (Linux / macOS / Windows)
- Hop geolocation via rDNS / IATA, optional MaxMind GeoLite2, and online fallbacks
- 3D globe path visualization (globe.gl / Three.js)
- First-run privacy notice and in-app About / privacy summary
- System tray with live tooltip
- Settings persistence (external-only, traces, local geo, history, density)
- GitHub Actions CI (frontend + multi-OS Rust) and release workflow for installers

### Notes

- Free ip-api.com batch lookups are HTTP-only; prefer a local GeoLite2 MMDB
- Unsigned builds may trigger OS trust warnings until code signing is configured

[0.1.0]: https://github.com/jonbng/network-cartographer/releases/tag/v0.1.0
