# Map My Network website and public API

This directory is an independent Next.js app for `mapmy.network`, designed to deploy on Vercel. It contains the product landing page, release launcher routes, and the server-only boundary for the future hosted geolocation service.

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
- `/api/v1/geo` — validates public IP batches and delegates to a server-side provider

Before announcing the commands, publish at least one tagged GitHub Release so the `latest/download` assets exist.

## Geo API

The provider adapter is intentionally disabled until a provider with suitable licensing is selected. Configure these server-only Vercel environment variables when it is ready:

```text
GEO_PROVIDER_URL=
GEO_PROVIDER_TOKEN=
```

The API accepts `POST { "ips": ["1.1.1.1"] }`, up to 40 addresses. It rejects private, loopback, link-local, multicast, reserved, and malformed addresses. The provider must return a JSON object with a `results` array matching the public response schema in `lib/geo/schema.ts`.

Before enabling it in production, add edge rate limiting, verify provider licensing, publish a retention policy, and add explicit opt-in consent to `netcart`. The CLI is not connected to this endpoint yet.
