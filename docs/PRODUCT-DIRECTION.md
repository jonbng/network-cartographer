# Map My Network — Product Direction

This document records the product and architecture decisions discussed in July 2026. It is the current direction, not a description of features that have all been implemented.

## Naming

Use different names for the product, command, and internal project where each serves a different purpose:

| Surface | Name | Reason |
|---------|------|--------|
| Public product | **Map My Network** | Clear, approachable, and easy to share |
| Primary domain | `mapmy.network` | Memorable and directly describes the product |
| CLI command | `netcart` | Short and pleasant to type while preserving the original identity |
| Repository / internal crate | `network-cartographer` | A good technical name that avoids unnecessary internal churn |
| Staging or fallback domain | `netcart.jonathanb.dk` | Useful for development, staging, or redirects |

The public message should be simple: “Run Map My Network.” The fact that the underlying project is called Network Cartographer does not need to be hidden, but it should not complicate the product experience.

## Distribution

The main trial experience should require one pasted command and no installed Node.js or Rust toolchain.

macOS and Linux:

```bash
curl -fsSL https://mapmy.network/run | sh
```

Windows PowerShell:

```powershell
irm https://mapmy.network/run.ps1 | iex
```

The launcher should:

1. Detect the operating system and CPU architecture.
2. Download the matching prebuilt binary from a GitHub Release.
3. Verify its published SHA-256 checksum.
4. Run it as the current user without requesting additional permissions.
5. Open the browser UI automatically.
6. Remove the temporary binary when the process exits.

The repository contains release-backed launchers under `scripts/`. The custom domain serves these routes through the public Next.js app and redirects to the matching checksummed GitHub Release launchers.

`mapmy.network` is a Next.js application deployed on Vercel. It provides a polished landing page with screenshots, a brief explanation, privacy information, source and release links, and the one-line commands. Useful routes include:

- `/` — landing page
- `/run` — macOS/Linux launcher
- `/run.ps1` — Windows launcher
- `/demo` — optional hosted demo using synthetic data
- `/source` — redirect to the public repository

Avoid making the bare domain return HTML or a shell script depending on request headers. Explicit launcher routes are easier to understand, audit, and debug.

## Runtime architecture

Map My Network is a CLI process with a browser UI, not an installed desktop application.

```text
netcart CLI
 ├─ reads local socket and process metadata
 ├─ performs normal-user traceroutes
 ├─ hosts a loopback-only HTTP API
 ├─ serves a version-matched embedded frontend
 └─ opens the user's default browser
```

The local server should remain bound to `127.0.0.1`. It must not be exposed to the local network or public internet by default.

### Keep the operational frontend embedded

The live frontend should continue to be embedded in the CLI binary and served locally. Do not make the production interface depend on JavaScript served from `mapmy.network`.

Reasons:

- The frontend can access sensitive local process and connection data.
- A compromised domain, CDN, deployment account, or frontend dependency would otherwise gain that access.
- The embedded frontend is version-matched to the local API.
- It works offline and avoids CORS, HTTPS-to-localhost, Private Network Access, and version-skew problems.

The public website may host a demo using fake data. That provides a zero-download preview without giving remote code access to a user's local monitor.

If a remote operational frontend is reconsidered later, it would require at minimum an exact-origin allowlist, random local port, one-time capability token on every request, no wildcard CORS, DNS-rebinding protection, and a careful browser compatibility review. Even then, it has a weaker trust model than an embedded frontend.

## Hosted geolocation

The ideal default experience should not require users to download large GeoLite databases. The public Vercel project owns the API boundary for a future zero-setup geolocation service:

```text
POST https://mapmy.network/api/v1/geo
```

The CLI should send batches containing only public hop and destination IP addresses. It must never send process names, executable paths, ports, host connection graphs, or other local application metadata to this service.

The response may contain:

- City and country
- Latitude and longitude
- ASN and organization
- Data source and confidence information

### Privacy and reliability requirements

- Ask for explicit first-run consent before any hosted lookup.
- Explain that public IP addresses may be sent to Map My Network for geolocation.
- Cache results locally so an address is normally queried once.
- Batch requests and apply service-side rate limits.
- Keep connection monitoring and process attribution entirely local.
- Minimize or disable infrastructure request logging where practical.
- Publish a clear retention policy.
- Keep local MMDB support as an optional offline/privacy mode.
- Handle service failure gracefully; the network monitor must remain useful without geolocation.

The privacy promise should be phrased accurately:

> Connection and process data remain local. Public IP addresses may be sent to Map My Network for geolocation when hosted lookups are enabled.

### Licensing caveat

Do not expose a GeoLite or commercial database through a public API until its license explicitly permits that use. Database redistribution and derived lookup services may be restricted. Before implementation, confirm the current provider terms or obtain a suitable commercial agreement. A licensed upstream GeoIP API may be safer than operating a public service directly from GeoLite data.

## Product principles

- One command should be enough to try the product.
- Do not require an account for local monitoring.
- Do not require additional permissions or privileged traceroute modes.
- Keep sensitive process and connection data on the user's machine.
- Treat the custom domain as the approachable product surface, not as a reason to weaken the local trust boundary.
- Prefer good defaults with optional privacy/offline controls over mandatory setup.

## Next steps

1. ✅ Rename public UI copy from Network Cartographer to Map My Network.
2. ✅ Rename the installed executable and CLI command to `netcart` while retaining internal repository naming.
3. ✅ The landing page, launcher routes, and Geo API boundary are implemented as a Next.js app under `site/`.
4. Create and test the first public GitHub Release so the one-command launchers work end to end.
5. Configure the Vercel project with `site/` as its root and attach `mapmy.network`.
6. Add a synthetic-data hosted demo.
7. Research GeoIP licensing and operating cost before configuring the provider adapter.
8. Add edge rate limiting and document the hosted geolocation retention policy and consent language before enabling it.
