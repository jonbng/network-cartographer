# Contributing

Thanks for helping improve Map My Network.

Please read the [Code of Conduct](./CODE_OF_CONDUCT.md). Security issues: [SECURITY.md](./SECURITY.md).

## Setup

See [README.md](./README.md) for prerequisites and:

```bash
npm install
npm start
```

## Guidelines

- Prefer small, focused pull requests.
- Match the existing TypeScript / Rust style in the files you touch.
- Do not commit build artifacts, secrets, or MaxMind `.mmdb` files.
- If you change traceroute or geo logic, note OS assumptions and any new network calls in the PR.
  Traceroute CLI flags are **per-OS** (`server/src/trace/engine.rs` → `commands_for`); keep every probe mode usable by a normal account.
- Run what you can locally: `npm run check` and `npm test`.
  CI runs Rust check/tests on Linux, macOS, and Windows — prefer keeping those green.
- `experiments/` is unsupported playground code; keep product changes out of that tree unless intentional.

## Releases

- Version is mirrored in `package.json` and `server/Cargo.toml`.
- Tag `vX.Y.Z` on `main` to trigger CLI binary builds.
- Update [CHANGELOG.md](./CHANGELOG.md) before tagging.

## Reporting bugs

Use the bug report template. Include the OS, browser, and Map My Network version shown in About.
