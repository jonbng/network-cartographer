# Map My Network website and public API

This directory is an independent Next.js app for `mapmy.network`, designed to deploy on Vercel. It contains the product landing page, release launcher routes, and the public geolocation API that proxies to a private VPS GeoLite lookup service.

## Preview

Install dependencies once, then run from the repository root:

```bash
npm --prefix site install
npm run site:dev
```

## Deploy

Import the repository into Vercel and set the project **Root Directory** to `site`. Next.js will be detected automatically. Add `mapmy.network` as the production domain.

The app exposes:

- `/` — product landing page
- `/run` — redirects to the latest checksummed macOS/Linux launcher
- `/run.ps1` — redirects to the latest checksummed Windows launcher
- `/source` — redirects to the public repository
- `/api/v1/geo` — validates public IP batches, rate-limits, caches, and proxies to the VPS

Before announcing the commands, publish at least one tagged GitHub Release so the `latest/download` assets exist.

## Hosted Geo API

Architecture:

```text
netcart  →  POST https://mapmy.network/api/v1/geo
         →  (validate + rate-limit + in-memory cache)
         →  GEO_PROVIDER_URL  (private VPS /v1/lookup)
```

### Vercel environment (server-only)

```text
GEO_PROVIDER_URL=https://<your-vps-or-tunnel>/v1/lookup
GEO_PROVIDER_TOKEN=<same secret as GEO_SERVICE_TOKEN on the VPS>
```

Never prefix these with `NEXT_PUBLIC_`.

### Behaviour

- Accepts `POST { "ips": ["1.1.1.1"] }`, up to 40 addresses
- Rejects private, loopback, link-local, multicast, reserved, and malformed addresses
- Rate limit: 60 requests / minute / client IP (429 when exceeded)
- Cache: successful city hits ~24h, misses ~1h (in-process; multi-instance cold misses are OK)
- Retention: lookup results may live in memory up to 24 hours; request bodies are not persisted
- `GET /api/v1/geo` reports `configured` / `unconfigured` plus a short privacy note

### VPS side

See [`../geo-service/README.md`](../geo-service/README.md) for the Rust lookup service, systemd unit, and weekly GeoLite refresh cron.

### CLI

After the privacy notice, `netcart` calls this endpoint by default. Staging override:

```text
NETWORK_CARTOGRAPHER_GEO_URL=https://staging.example/api/v1/geo
```

Local MMDB + **Local geolocation** in the UI remains the offline path.
