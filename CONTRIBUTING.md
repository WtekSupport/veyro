# Contributing to Veyro

Thank you for your interest in Veyro. This project welcomes pull requests from the community.

## Before you start

- **Security issues:** do not open public issues. Use [GitHub Security Advisories](https://github.com/WtekSupport/veyro/security/advisories/new) (see [`.github/SECURITY.md`](.github/SECURITY.md)).
- **Questions / ideas:** open a [GitHub issue](https://github.com/WtekSupport/veyro/issues) (bug or feature template).
- **Website:** the marketing site lives in [WtekSupport/veyroland](https://github.com/WtekSupport/veyroland) (production: [veyrotext.vercel.app](https://veyrotext.vercel.app)).

## Development setup

1. Fork and clone `https://github.com/WtekSupport/veyro.git`.
2. Install [Node.js](https://nodejs.org/) 18+ and [Rust](https://www.rust-lang.org/tools/install) (stable).
3. From the repo root:

```bash
npm ci
npm run prepare   # installs git hooks (co-author trailer checks)
```

4. **Linux (CI-like check):**

```bash
cd src-tauri
cargo test --no-default-features --lib
cargo clippy --no-default-features --lib -- -D warnings
```

5. **Full desktop build** (Whisper/LLM, GPU) is maintainer-tested on Windows/macOS. See [docs/BUILD.en.md](docs/BUILD.en.md) and [docs/BUILD.md](docs/BUILD.md).

## Pull requests

1. Branch from `main` (e.g. `fix/…`, `feat/…`).
2. Keep changes focused; match existing code style.
3. Ensure CI passes (`frontend` and `rust` jobs). `main` is protected: green checks required.
4. **Commit messages:** human author only. Do **not** add `Co-authored-by:` trailers (Cursor/Claude/bots). Local hooks enforce this.
5. Open a PR against `main` and fill out the template.

Maintainers review and merge. Avoid force-pushing `main` after external forks exist.

## Git history note

`main` was squashed to a clean root history for release hygiene. New contributions use normal commits on top; blame for older lines may be limited.

## Changelog and releases

User-facing release notes are published on [GitHub Releases](https://github.com/WtekSupport/veyro/releases). Maintainers update [CHANGELOG.md](CHANGELOG.md) when cutting a release (see [docs/RELEASE.md](docs/RELEASE.md)).

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
