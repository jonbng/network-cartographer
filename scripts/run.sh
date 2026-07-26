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
    fail "This device is not supported yet."
    ;;
esac

case "$(uname -m)" in
  x86_64|amd64) architecture="x86_64" ;;
  arm64|aarch64) architecture="arm64" ;;
  *)
    fail "This device is not supported yet."
    ;;
esac

archive="network-cartographer-${platform}-${architecture}.tar.gz"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/network-cartographer.XXXXXX")"
cleanup() {
  if [ -d "$work_dir" ]; then
    rm -rf "$work_dir"
  fi
}
# The app handles Ctrl+C itself. EXIT cleanup runs after that graceful stop.
trap cleanup 0

printf '\n%s\n' "Network Cartographer"
printf '%s\n' "---------------------"
if command -v curl >/dev/null 2>&1; then
  download_progress() { curl -fL --progress-bar "$1" -o "$2"; }
  download_quiet() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  download_progress() { wget -q --show-progress "$1" -O "$2"; }
  download_quiet() { wget -q "$1" -O "$2"; }
else
  fail "Network Cartographer needs curl or wget to start."
fi
printf '  %-12s %s\n' "Status" "Starting"
download_progress "${release_base}/${archive}" "${work_dir}/${archive}" || fail "Could not start Network Cartographer. Check your connection and try again."
download_quiet "${release_base}/SHA256SUMS" "${work_dir}/SHA256SUMS" || fail "Could not start Network Cartographer. Check your connection and try again."

expected_hash="$(awk -v file="$archive" '$2 == file { print $1 }' "${work_dir}/SHA256SUMS")"
if [ -z "$expected_hash" ]; then
  fail "Network Cartographer could not be verified. Please try again."
fi
if command -v sha256sum >/dev/null 2>&1; then
  actual_hash="$(sha256sum "${work_dir}/${archive}" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual_hash="$(shasum -a 256 "${work_dir}/${archive}" | awk '{ print $1 }')"
else
  fail "Network Cartographer could not be verified on this device."
fi
if [ "$actual_hash" != "$expected_hash" ]; then
  fail "Network Cartographer could not be verified. Please try again."
fi

tar -xzf "${work_dir}/${archive}" -C "$work_dir" || fail "Network Cartographer could not be prepared. Please try again."
binary="${work_dir}/network-cartographer-${platform}-${architecture}/netcart"
if [ ! -f "$binary" ]; then
  fail "Network Cartographer could not be prepared. Please try again."
fi
chmod +x "$binary"

set +e
NETCART_LAUNCHED=1 "$binary" "$@"
status=$?
set -e

cleanup
trap - 0
printf '  %-12s %s\n' "Cleanup" "temporary files removed"
printf '  %-12s %s\n\n' "Done" "Network Cartographer stopped"
exit "$status"
