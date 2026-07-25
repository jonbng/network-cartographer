# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Hosted geolocation via `mapmy.network/api/v1/geo` (self-hosted GeoLite on a private VPS) replaces free `ip-api.com` / `ipwho.is` fallbacks
- Privacy copy and docs describe Map My Network hosted geo; local MMDB remains an offline opt-in

## [0.1.1] — 2026-07-25

### Fixed

- Close the browser's live event stream during graceful shutdown so Ctrl+C exits immediately

## [0.1.0] — 2026-07-24

### Added

- Local CLI with a browser-based globe and live per-app TCP connection monitoring
- Automatic traceroute queue with per-OS probe methods (Linux / macOS / Windows)
- Hop geolocation via rDNS / IATA, optional MaxMind GeoLite2, and online fallbacks
- 3D globe path visualization (globe.gl / Three.js)
- First-run privacy notice and in-app About / privacy summary
- Loopback-only HTTP API with live Server-Sent Events
- One-command launchers backed by prebuilt cross-platform release binaries
- Settings persistence (external-only, traces, local geo, history, density)
- GitHub Actions CI (frontend + multi-OS Rust) and release workflow for CLI binaries
- Attribution-quality statistics and a separate Unattributed traffic section
- Opt-in Linux per-application upload/download rates using unprivileged kernel socket diagnostics

### Fixed

- Preserve exact socket ownership while TCP connections transition through teardown states
- Count each socket once instead of treating every polling observation as another request
- Avoid merging every PID-less connection into a misleading `unknown` application

### Notes

- Free ip-api.com batch lookups are HTTP-only; prefer a local GeoLite2 MMDB
- The CLI uses normal-user traceroute modes and never requests additional permissions

[0.1.0]: https://github.com/jonbng/network-cartographer/releases/tag/v0.1.0
[0.1.1]: https://github.com/jonbng/network-cartographer/compare/v0.1.0...v0.1.1
