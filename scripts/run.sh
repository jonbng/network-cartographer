#!/bin/sh
# Download the latest Network Cartographer release for this machine and run it.
set -eu

repository="jonbng/network-cartographer"
release_base="https://github.com/${repository}/releases/latest/download"

fail() {
  printf '\nError: %s\n' "$1" >&2
  exit 1
}

case "$(uname -s)" in
  Linux) platform="linux" ;;
  Darwin) platform="macos" ;;
  *)
    fail "This launcher supports Linux and macOS. Windows uses run.ps1."
    ;;
esac

case "$(uname -m)" in
  x86_64|amd64) architecture="x86_64" ;;
  arm64|aarch64) architecture="arm64" ;;
  *)
    fail "Unsupported CPU architecture: $(uname -m)"
    ;;
esac

archive="network-cartographer-${platform}-${architecture}.tar.gz"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/network-cartographer.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT HUP INT TERM

printf '\n%s\n' "Network Cartographer"
printf '  Preparing the latest release for %s/%s\n\n' "$platform" "$architecture"
if command -v curl >/dev/null 2>&1; then
  download() { curl -fL --progress-bar "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  download() { wget -q --show-progress "$1" -O "$2"; }
else
  fail "curl or wget is required to download the release."
fi
printf '%s\n' "[1/3] Downloading…"
download "${release_base}/${archive}" "${work_dir}/${archive}" || fail "Could not download ${archive}."
download "${release_base}/SHA256SUMS" "${work_dir}/SHA256SUMS" || fail "Could not download the release checksum."

printf '%s\n' "[2/3] Verifying…"
expected_hash="$(awk -v file="$archive" '$2 == file { print $1 }' "${work_dir}/SHA256SUMS")"
if [ -z "$expected_hash" ]; then
  fail "No checksum was published for ${archive}."
fi
if command -v sha256sum >/dev/null 2>&1; then
  actual_hash="$(sha256sum "${work_dir}/${archive}" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual_hash="$(shasum -a 256 "${work_dir}/${archive}" | awk '{ print $1 }')"
else
  fail "sha256sum or shasum is required to verify the download."
fi
if [ "$actual_hash" != "$expected_hash" ]; then
  fail "The downloaded release failed checksum verification."
fi

tar -xzf "${work_dir}/${archive}" -C "$work_dir" || fail "Could not unpack the downloaded release."
binary="${work_dir}/network-cartographer-${platform}-${architecture}/netcart"
if [ ! -f "$binary" ]; then
  fail "The downloaded release did not contain the expected binary."
fi
chmod +x "$binary"

printf '%s\n\n' "[3/3] Starting…"
set +e
"$binary" "$@"
status=$?
set -e
exit "$status"
