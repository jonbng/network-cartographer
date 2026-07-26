# mapmy-geo (private VPS lookup service)

Always-on GeoLite2 City + ASN lookup binary for Network Cartographer. **Not** a public API; only `mapmy.network` (Vercel) should call it with a shared bearer token.

## Endpoints

| Method | Path | Auth | Purpose |
|--------|------|------|---------|
| `GET` | `/healthz` | none | Liveness + whether MMDBs loaded |
| `POST` | `/v1/lookup` | Bearer | `{ "ips": ["1.1.1.1"] }` → `{ "results": [...] }` |
| `POST` | `/v1/reload` | Bearer | Re-open MMDB files after a weekly refresh |

## Environment

| Variable | Required | Default | Notes |
|----------|----------|---------|-------|
| `GEO_SERVICE_TOKEN` | yes | - | ≥16 chars; same value as Vercel `GEO_PROVIDER_TOKEN` |
| `GEO_CITY_MMDB` | no | `data/GeoLite2-City.mmdb` | City database path |
| `GEO_ASN_MMDB` | no | `data/GeoLite2-ASN.mmdb` | ASN database path |
| `GEO_LISTEN` | no | `127.0.0.1:8787` | Bind address (keep private) |

## Local run

```bash
# From repo root, after downloading GeoLite2 into ./data
export GEO_SERVICE_TOKEN="$(openssl rand -hex 24)"
export GEO_CITY_MMDB="$PWD/data/GeoLite2-City.mmdb"
export GEO_ASN_MMDB="$PWD/data/GeoLite2-ASN.mmdb"
cargo run --manifest-path geo-service/Cargo.toml --release
```

## Deploy sketch

1. Build: `cargo build --manifest-path geo-service/Cargo.toml --release`
2. Copy `target/release/mapmy-geo`, `deploy/mapmy-geo.service`, and `deploy/update-and-reload.sh` to `/opt/mapmy-geo/`
3. Place `.env` with `GEO_SERVICE_TOKEN`, `MAXMIND_LICENSE_KEY`, and MMDB paths
4. Enable the systemd unit; firewall so only your Vercel egress / Tailscale / SSH can reach the port (or terminate TLS on localhost via Caddy and allowlist)
5. Weekly cron: `deploy/update-and-reload.sh`

See [`../site/README.md`](../site/README.md) for the Vercel proxy side.
