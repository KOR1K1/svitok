# Contributing to Svitok

Thanks for taking a look. Bug reports, audits of the crypto, translations, and small focused PRs are all welcome. Please open an issue before starting anything large so we don't build the same thing two different ways.

## The one hard rule: don't break the paper

Svitok's whole promise is that a seed and phrase written down today still produce the same passwords years from now, on any machine. That means **the derivation scheme is frozen.**

`core/tests/golden.rs` pins the exact output of the master key, per-site password, fingerprint, and the paper round-trip. If a change makes those tests fail, it has broken bit-compatibility, and every seed already written on paper becomes worthless. So:

- Never change constants, domain-separation strings, byte order, or the KDF/derivation math in a way that alters output.
- KDF *parameters* (`M`, `T`) are allowed to grow, because they're written on the paper next to the seed - new seeds can use stronger defaults while old ones keep reading with their own values. Changing the *default* is fine; changing the *algorithm* is not.
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

## Where help is most useful

- **Auditing `core/`** - the hand-rolled crypto. This is the important stuff. If you're a cryptographer, please be mean to it.
- **Reproducible builds** - the biggest trust gap right now.
- **Windows clipboard** - excluding copied passwords from clipboard history / Cloud Clipboard needs a native path (Android already marks them sensitive).
- **Translations** - strings live in `app/src/i18n.ts`, two flat dictionaries (`ru`, `en`). Add a language by copying one.
- **F-Droid metadata**, docs, and screenshots.

## Style

- Match the code around you - naming, spacing, how errors are handled. Nothing exotic.
- Comments explain *why*, not *what*. If the code already says it, don't add a comment. Write them like a person did: plain language, a regular hyphen instead of an em-dash, no emoji, no "note that" / "this function does X" filler.
- No new dependencies in `core/` - it's zero-dependency on purpose. Elsewhere, add a dependency only if it really earns its place.
- Keep secrets out of the JS/IPC layer. The master key and seed live in Rust; only derived results and metadata cross the bridge. Wipe key material when you're done with it (`svitok_core::wipe`).

## Commits and PRs

- Small, focused commits with a clear message. Present tense is fine ("add X", "fix Y").
- One logical change per PR. If it touches the crypto core, say so up front and show that the golden vectors still pass.
- If you used an AI tool for a substantial chunk, just mention it in the PR - no big deal, it's just useful to know.

## Security issues

Don't file public issues for vulnerabilities. Open a private security advisory on GitHub (or email the maintainer). See [README#security](README.md#security).

## License

By contributing, you agree your work is licensed under [GPL-3.0-or-later](LICENSE), same as the rest of the project.
