# Network Cartographer website and public API

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
- `/api/v1/egress` — locates the public client address observed by the hosting edge
- `/api/v1/runs` — increments and reads the anonymous aggregate run count in Upstash Redis

Before announcing the commands, publish at least one tagged GitHub Release so the `latest/download` assets exist.

## Hosted Geo API

Architecture:

```text
netcart  →  POST https://mapmy.network/api/v1/geo
         →  GET  https://mapmy.network/api/v1/egress
         →  (validate + rate-limit + in-memory cache)
         →  GEO_PROVIDER_URL  (private VPS /v1/lookup)
```

### Vercel environment (server-only)

```text
GEO_PROVIDER_URL=https://<your-vps-or-tunnel>/v1/lookup
GEO_PROVIDER_TOKEN=<same secret as GEO_SERVICE_TOKEN on the VPS>
```

Never prefix these with `NEXT_PUBLIC_`.

### Run counter

Add the Upstash REST credentials to the Vercel project:

```text
UPSTASH_REDIS_REST_URL=https://<database>.upstash.io
UPSTASH_REDIS_REST_TOKEN=<rest-token>
```

Successful release-build startups make a best-effort `POST /api/v1/runs`. The endpoint atomically increments `network-cartographer:runs:total`; `GET /api/v1/runs` returns the current count. It stores no client identifiers or request metadata in Redis and limits each observed client address to five increments per minute. Development builds do not report, and users can opt out with `NETCART_DISABLE_USAGE_PING=1`.

### Behaviour

- Accepts `POST { "ips": ["1.1.1.1"] }`, up to 40 addresses
- `GET /api/v1/egress` accepts no caller-supplied address; it validates and locates the public source address observed by the hosting edge
- Rejects private, loopback, link-local, multicast, reserved, and malformed addresses
- Rate limit: 60 requests / minute / client IP (429 when exceeded)
- Cache: successful city hits ~24h, misses ~1h (in-process; multi-instance cold misses are OK)
- Retention: lookup results may live in memory up to 24 hours; request bodies are not persisted
- `GET /api/v1/geo` reports `configured` / `unconfigured` plus a short privacy note

### VPS side

See [`../geo-service/README.md`](../geo-service/README.md) for the Rust lookup service, systemd unit, and weekly GeoLite refresh cron.

### CLI

`netcart` calls this endpoint by default. Staging override:

```text
NETWORK_CARTOGRAPHER_GEO_URL=https://staging.example/api/v1/geo
NETWORK_CARTOGRAPHER_EGRESS_URL=https://staging.example/api/v1/egress
```

Local MMDB + **Local geolocation** in the UI remains the offline path.
