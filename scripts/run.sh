#!/bin/sh
# Download the latest Map My Network release for this machine and run it.
set -eu

repository="jonbng/network-cartographer"
release_base="https://github.com/${repository}/releases/latest/download"

case "$(uname -s)" in
  Linux) platform="linux" ;;
  Darwin) platform="macos" ;;
  *)
    printf '%s\n' "Map My Network supports Linux, macOS, and Windows." >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  x86_64|amd64) architecture="x86_64" ;;
  arm64|aarch64) architecture="arm64" ;;
  *)
    printf 'Unsupported CPU architecture: %s\n' "$(uname -m)" >&2
    exit 1
    ;;
esac

archive="map-my-network-${platform}-${architecture}.tar.gz"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/map-my-network.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT HUP INT TERM

printf 'Downloading Map My Network for %s/%s…\n' "$platform" "$architecture"
if command -v curl >/dev/null 2>&1; then
  download() { curl -fL --progress-bar "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  download() { wget -q --show-progress "$1" -O "$2"; }
else
  printf '%s\n' "This launcher needs curl or wget." >&2
  exit 1
fi
download "${release_base}/${archive}" "${work_dir}/${archive}"
download "${release_base}/SHA256SUMS" "${work_dir}/SHA256SUMS"

expected_hash="$(awk -v file="$archive" '$2 == file { print $1 }' "${work_dir}/SHA256SUMS")"
if [ -z "$expected_hash" ]; then
  printf '%s\n' "No checksum was published for ${archive}." >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  actual_hash="$(sha256sum "${work_dir}/${archive}" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual_hash="$(shasum -a 256 "${work_dir}/${archive}" | awk '{ print $1 }')"
else
  printf '%s\n' "Could not verify the download: sha256sum or shasum is required." >&2
  exit 1
fi
if [ "$actual_hash" != "$expected_hash" ]; then
  printf '%s\n' "The downloaded release failed checksum verification." >&2
  exit 1
fi

tar -xzf "${work_dir}/${archive}" -C "$work_dir"
binary="${work_dir}/map-my-network-${platform}-${architecture}/netcart"
if [ ! -f "$binary" ]; then
  printf '%s\n' "The downloaded release did not contain the expected binary." >&2
  exit 1
fi
chmod +x "$binary"

set +e
"$binary" "$@"
status=$?
set -e
exit "$status"
