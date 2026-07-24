---
name: release
version: 1.0.0
disable-model-invocation: true
description: |
  Cut a Svitok release. Only runs when the user types /release - never auto-invoked,
  because it pushes a tag and publishes.
allowed-tools:
  - Bash
  - Read
---

# release

You were invoked with `/release`. Drive the release; pause for the human on anything
irreversible (tag push, publish). The workflow is `.github/workflows/release.yml`:
a pushed `vX.Y.Z` tag builds desktop bundles, renames them to
`Svitok-VERSION-platform.ext`, and writes `SHA256SUMS.txt`.

## Steps

1. **Bump versions** consistently: `app/src-tauri/Cargo.toml`, `tauri.conf.json`, and
   the Android `versionCode`/`versionName`. Confirm which crates need it - don't bump
   blindly.
2. **Golden vectors and lints:** `cargo test --workspace` must pass. If crypto was
   touched this cycle, invoke `crypto-guard` first.
3. **Write the release notes as the annotated tag message, in our own voice.** The
   workflow pulls notes straight from the tag annotation
   (`git tag -l --format='%(contents)'`), so this text *is* the GitHub release body.
   Write it the way past releases read - from us, not from CI. Apply `anti-slop`
   (plain language, regular hyphen, no emoji, no filler). Group user-facing changes;
   mention the issues closed.
   ```bash
   git tag -a vX.Y.Z          # opens an editor for the annotated message
   ```
4. **Push the tag** (this triggers the build - confirm with the user first):
   ```bash
   git push origin vX.Y.Z
   ```
   Note the release job does `git fetch --force ...refs/tags/...` so the real
   annotated tag (not the checkout-peeled one) supplies the notes - don't "fix" that.
5. **Android APK is built and signed locally**, then attached to the release and its
   checksum appended to `SHA256SUMS.txt` (the desktop workflow leaves a placeholder
   asking for exactly this). Build it reproducibly and run `verify-reproducible`
   (including the no-INTERNET check) before attaching.
6. **Finalize and publish** the GitHub release once desktop bundles, the signed APK,
   and the full `SHA256SUMS.txt` are all present. Don't publish a half-filled draft.

## Don'ts

- No AI attribution anywhere in the tag message or release notes.
- Don't publish before the APK and its checksum are in - a release missing Android is
  not done.
- Solo release: the maintainer holds the signing key; the signed APK step is theirs.
