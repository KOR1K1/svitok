# Contributing to Svitok

Thanks for taking a look. Bug reports, audits of the crypto, translations, and small focused PRs are all welcome. Please open an issue before starting anything large so we don't build the same thing two different ways.

## The one hard rule: don't break the paper

Svitok's whole promise is that a seed written down today (plus the phrase you keep in your head) still produces the same passwords years from now, on any machine. That means **the derivation scheme is frozen.**

`core/tests/golden.rs` pins the exact output of the master key, per-site password, fingerprint, and the paper round-trip. If a change makes those tests fail, it has broken bit-compatibility, and every seed already written on paper becomes worthless. So:

- Never change constants, domain-separation strings, byte order, or the KDF/derivation math in a way that alters output.
- KDF *parameters* (`M`, `T`) are allowed to grow, because they're written on the paper next to the seed - new seeds can use stronger defaults while old ones keep reading with their own values. Changing the *default* is fine; changing the *algorithm* is not.
- Anything added for matching or organizing - the entry id, alias domains, the display label - is metadata and must **never** become an input to derivation. The password is a function of site, login, counter and policy, nothing else. If a feature needs more inputs to `F`, it's the wrong design for this project.
- If you think the scheme itself has a real flaw, that's an issue to discuss, not a quiet PR.

Run the vectors before and after your change:

```bash
cargo test --workspace
```

## Setup

See the [Build from source](README.md#build-from-source) section in the README. Short version:

```bash
cargo test --workspace       # crypto + storage + QR
cargo run -p svitok -- --help   # CLI, easiest way to poke the algorithm
cd app && npm install && npm run tauri dev   # the GUI
```

Heads up: `--workspace` includes the Tauri crate, which on Linux needs the system
packages from the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
(webkit2gtk, libsecret, ...). Without them, run what CI runs - it's the whole
crypto/golden suite:

```bash
cargo test -p svitok-core -p svitok-common -p svitok-cli
```

## Where help is most useful

- **Auditing `core/`** - the hand-rolled crypto. This is the important stuff. If you're a cryptographer, please be mean to it.
- **Reproducible builds** - the recipe is in [`docs/REPRODUCIBLE.md`](docs/REPRODUCIBLE.md); independent verification runs on other machines are exactly the help this needs.
- **Autofill** - registering the desktop native host from the installer, and matching native apps (not just web domains).
- **Translations** - strings live in `app/src/i18n.ts`, two flat dictionaries (`ru`, `en`). Add a language by copying one.
- **F-Droid metadata**, docs, and screenshots.

## Style

- Match the code around you - naming, spacing, how errors are handled. Nothing exotic.
- Comments explain *why*, not *what*. If the code already says it, don't add a comment. Write them like a person did: plain language, a regular hyphen instead of an em-dash, no emoji, no "note that" / "this function does X" filler.
- No new dependencies in `core/` - it's zero-dependency on purpose. Elsewhere, add a dependency only if it really earns its place.
- Keep secrets out of the JS/IPC layer. The master key lives in Rust and never crosses the bridge; the seed crosses only as paper lines on an explicit user action (`create_vault`, `show_seed`). Everything else is derived results and metadata. Wipe key material when you're done with it (`svitok_core::wipe`).

## Commits and PRs

- Small, focused commits with a clear message. Present tense is fine ("add X", "fix Y").
- One logical change per PR. If it touches the crypto core, say so up front and show that the golden vectors still pass.
- If you used an AI tool for a substantial chunk, just mention it in the PR - no big deal, it's just useful to know.

## Cutting a release (maintainer notes)

- The version lives in `app/src-tauri/Cargo.toml`, `app/src-tauri/tauri.conf.json`,
  and the Android `versionCode`/`versionName` in `gen/android/app/build.gradle.kts`.
  The library crates and `package.json` keep their own numbers on purpose - don't
  "sync" them.
- Pushing an **annotated** tag `vX.Y.Z` triggers the release workflow; the tag
  annotation becomes the GitHub release body, so write the notes there. A lightweight
  tag means an empty release page.
- CI builds the desktop bundles and `SHA256SUMS.txt`. The Android APK is built and
  signed locally by the maintainer, attached to the release, and its checksum
  appended - the release isn't done until that happens.

## Security issues

Don't file public issues for vulnerabilities. Open a private security advisory on GitHub (or email the maintainer). See [README#security](README.md#security).

## License

By contributing, you agree your work is licensed under [GPL-3.0-or-later](LICENSE), same as the rest of the project.
