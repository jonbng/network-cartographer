#!/usr/bin/env bash
# Weekly cron helper: refresh GeoLite2 DBs and ask the service to reload readers.
# Example crontab (as the service user):
#   15 4 * * 1 /opt/mapmy-geo/update-and-reload.sh >>/var/log/mapmy-geo-update.log 2>&1
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# When installed to /opt/mapmy-geo, prefer that layout:
if [[ -d /opt/mapmy-geo ]]; then
  ROOT=/opt/mapmy-geo
fi

export MAXMIND_LICENSE_KEY="${MAXMIND_LICENSE_KEY:-}"
if [[ -z "${MAXMIND_LICENSE_KEY}" && -f "${ROOT}/.env" ]]; then
  # shellcheck disable=SC1091
  set -a
  source "${ROOT}/.env"
  set +a
fi

if [[ -z "${MAXMIND_LICENSE_KEY:-}" ]]; then
  echo "MAXMIND_LICENSE_KEY is required" >&2
  exit 1
fi

OUT="${ROOT}/data"
mkdir -p "$OUT"

download() {
  local edition="$1"
  local dest="$2"
  local url="https://download.maxmind.com/app/geoip_download?edition_id=${edition}&license_key=${MAXMIND_LICENSE_KEY}&suffix=tar.gz"
  local tmp
  tmp="$(mktemp -d)"
  echo "Downloading ${edition}..."
  curl -fsSL "$url" -o "${tmp}/db.tar.gz"
  tar -xzf "${tmp}/db.tar.gz" -C "$tmp"
  local mmdb
  mmdb="$(find "$tmp" -name '*.mmdb' | head -1)"
  if [[ -z "$mmdb" ]]; then
    echo "No .mmdb found in archive for ${edition}" >&2
    exit 1
  fi
  cp "$mmdb" "$dest"
  echo "Wrote $dest"
  rm -rf "$tmp"
}

download "GeoLite2-City" "${OUT}/GeoLite2-City.mmdb"
download "GeoLite2-ASN" "${OUT}/GeoLite2-ASN.mmdb"

TOKEN="${GEO_SERVICE_TOKEN:-}"
LISTEN_HOST="${GEO_RELOAD_URL:-http://127.0.0.1:8787/v1/reload}"
if [[ -n "$TOKEN" ]]; then
  curl -fsS -X POST \
    -H "Authorization: Bearer ${TOKEN}" \
    "${LISTEN_HOST}" >/dev/null
  echo "Reloaded geo service readers"
else
  echo "GEO_SERVICE_TOKEN unset; databases updated; restart mapmy-geo manually"
fi
