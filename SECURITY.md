# Security policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## What Map My Network does

Map My Network is a **local** network connection monitor. It reads OS socket tables and process metadata, runs normal-user traceroute tools, and may send **IP addresses** to third-party GeoIP APIs unless a local MaxMind database is configured.

It does **not** operate a Map My Network backend that receives your connection lists or process names.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security-sensitive reports.

Prefer one of:

1. **GitHub Security Advisories** on [jonbng/network-cartographer](https://github.com/jonbng/network-cartographer/security/advisories/new) (private report), or
2. Email the maintainer listed in `Cargo.toml` / GitHub profile.

Include:

- Affected version or commit
- OS, browser, and command-line options
- Description of the issue and impact
- Steps to reproduce (PoC if possible)

You should receive an acknowledgment within a few days when possible. We will coordinate a fix and disclosure timeline with you.

## Scope examples

**In scope**

- Local API authorization bypass or DNS-rebinding issues
- Path traversal or arbitrary code execution via app inputs
- Unintended exfiltration of process names, full connection tables, or local files
- Cross-origin access to the loopback HTTP API

**Out of scope**

- Inaccuracy of third-party GeoIP or reverse DNS
- Rate limits or ToS of external GeoIP services
- OS traceroute tool behavior itself
- Behavior caused by unsupported third-party traceroute variants

## Hardening tips for users

- Prefer a local GeoLite2 City (and optional ASN) database: set `NETWORK_CARTOGRAPHER_MMDB` / `NETWORK_CARTOGRAPHER_ASN_MMDB` or place files under standard GeoIP paths (see README)
- Enable **Local geo** in the UI when an MMDB is available to reduce online lookups
- Keep the CLI bound to its default loopback address; do not place it behind a public proxy
