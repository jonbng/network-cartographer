# Security policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## What hopglobe does

hopglobe is a **local** network connection monitor. It reads OS socket tables and process metadata, runs traceroute tools, and may send **IP addresses** to third-party GeoIP APIs unless a local MaxMind database is configured.

It does **not** operate a hopglobe backend that receives your connection lists or process names.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security-sensitive reports.

Prefer one of:

1. **GitHub Security Advisories** on [jonbng/hopglobe](https://github.com/jonbng/hopglobe/security/advisories/new) (private report), or
2. Email the maintainer listed in `Cargo.toml` / GitHub profile.

Include:

- Affected version or commit
- OS and whether the app was elevated
- Description of the issue and impact
- Steps to reproduce (PoC if possible)

You should receive an acknowledgment within a few days when possible. We will coordinate a fix and disclosure timeline with you.

## Scope examples

**In scope**

- Privilege escalation or unexpected elevated behavior
- Path traversal or arbitrary code execution via app inputs
- Unintended exfiltration of process names, full connection tables, or local files
- CSP / IPC permission bypass in the Tauri webview

**Out of scope**

- Inaccuracy of third-party GeoIP or reverse DNS
- Rate limits or ToS of external GeoIP services
- OS traceroute tool behavior itself
- Issues that only apply when the user intentionally runs as root/admin

## Hardening tips for users

- Prefer a local GeoLite2 City (and optional ASN) database: set `HOPGLOBE_MMDB` / `HOPGLOBE_ASN_MMDB` or place files under standard GeoIP paths (see README)
- Enable **Local geo** in the UI when an MMDB is available to reduce online lookups
- Run elevated only when needed (process attribution / privileged traceroute methods)
