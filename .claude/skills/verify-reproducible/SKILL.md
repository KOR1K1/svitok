---
name: verify-reproducible
version: 1.0.0
description: |
  Verify a Svitok build is reproducible and clean - desktop hash against
  SHA256SUMS.txt, Android via apksigcopier, and the no-INTERNET-permission check on
  the APK. Use when asked to "verify the build", "check reproducibility", "проверь
  сборку", or before publishing a release APK.
allowed-tools:
  - Bash
  - Read
---

# verify-reproducible

Full recipe with pinned toolchains and exact flags is in
[docs/REPRODUCIBLE.md](../../docs/REPRODUCIBLE.md) - follow it, don't reinvent the
flags. This skill is the checklist.

## Desktop

1. Build with the deterministic `RUSTFLAGS` from the doc (`--remap-path-prefix` for
   repo and cargo home; `/Brepro` link-arg on Windows).
2. Hash the portable executable and compare to its line in the release
   `SHA256SUMS.txt`:
   ```bash
   sha256sum target/release/svitok-app.exe    # or Get-FileHash on PowerShell
   ```
   Verify the portable exe, not the NSIS installer (installer isn't byte-reproducible
   yet).

## Android

1. Build unsigned with the doc's flags/toolchains -> `app-universal-release-unsigned.apk`.
2. Confirm it matches the released signed APK except for the signature:
   ```bash
   apksigcopier compare Svitok-VERSION-android.apk --unsigned app-universal-release-unsigned.apk
   ```
   Silence = identical. On Windows where the `apksigner` shim isn't found, graft and
   hash-compare instead (see the doc).

## The no-INTERNET check (regression guard)

Svitok must ship with **no** `android.permission.INTERNET`. It regressed once when a
Google ML Kit dependency pulled it in via manifest merge; it's now stripped in the
release manifest with `tools:node="remove"`. Always verify on the **built** APK, not
the source manifest:

```bash
aapt2 dump permissions app-universal-release-unsigned.apk | grep -i internet
```

Empty output is correct. Any `INTERNET` line means the regression is back - do not
publish; find what dependency reintroduced it. (`aapt2` lives in the Android SDK
build-tools.)
