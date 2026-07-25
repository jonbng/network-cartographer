# Contributing

Thanks for helping improve hopglobe.

Please read the [Code of Conduct](./CODE_OF_CONDUCT.md). Security issues: [SECURITY.md](./SECURITY.md).

## Setup

See [README.md](./README.md) for prerequisites and:

```bash
npm install
npm run tauri dev
```

## Guidelines

- Prefer small, focused pull requests.
- Match the existing TypeScript / Rust style in the files you touch.
- Do not commit build artifacts, secrets, or MaxMind `.mmdb` files.
- If you change traceroute or geo logic, note OS assumptions and any new network calls in the PR.
  Traceroute CLI flags are **per-OS** (`src-tauri/src/trace/engine.rs` → `commands_for`); do not reuse Linux-only options (`-T`, `-N`, `tracepath`) on macOS.
- Run what you can locally: `npm run build`, and in `src-tauri/` run `cargo test --locked` / `cargo check --locked`.
  CI runs Rust check/tests on Linux, macOS, and Windows — prefer keeping those green.
- `experiments/` is unsupported playground code; keep desktop app changes out of that tree unless intentional.

## Releases

- Version is mirrored in `package.json` and `src-tauri/tauri.conf.json` (and `Cargo.toml`).
- Tag `vX.Y.Z` on `main` to trigger `.github/workflows/release.yml` (draft GitHub Release + installers).
- Update [CHANGELOG.md](./CHANGELOG.md) before tagging.

## Reporting bugs

Use the bug report template. Include OS, hopglobe version (footer / About), and whether the app was elevated.
