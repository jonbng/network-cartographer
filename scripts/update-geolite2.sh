#!/usr/bin/env bash
# Download GeoLite2 City + ASN into ./data (gitignored).
# Requires: MAXMIND_LICENSE_KEY in the environment (never commit the key).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${ROOT}/data"
mkdir -p "$OUT"

if [[ -z "${MAXMIND_LICENSE_KEY:-}" ]]; then
  echo "Set MAXMIND_LICENSE_KEY to your MaxMind license key." >&2
  echo "https://www.maxmind.com/en/accounts/current/license-key" >&2
  exit 1
fi

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

# Also copy to project root for convenience
cp -f "${OUT}/GeoLite2-City.mmdb" "${ROOT}/GeoLite2-City.mmdb" 2>/dev/null || true
cp -f "${OUT}/GeoLite2-ASN.mmdb" "${ROOT}/GeoLite2-ASN.mmdb" 2>/dev/null || true

echo "Done. Restart hopglobe to load the databases."
echo "Optional: export HOPGLOBE_MMDB=${OUT}/GeoLite2-City.mmdb"
echo "Optional: export HOPGLOBE_ASN_MMDB=${OUT}/GeoLite2-ASN.mmdb"
