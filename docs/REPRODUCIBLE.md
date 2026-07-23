# Reproducible builds

The promise: build Svitok from source yourself and get byte-identical binaries
to the ones in the release. Then you're trusting the code you can read, not the
person who ran the build.

## Pinned toolchains

| What | Version | Pinned where |
| --- | --- | --- |
| Rust | 1.95.0 | `rust-toolchain.toml` (rustup picks it up automatically) |
| Node.js | 22 | `.nvmrc`; CI uses the same file |
| JDK (Android) | Temurin 21 | documented here; use exactly this distribution |
| Android NDK | 28.2.13676358 | `ndkVersion` in `app/src-tauri/gen/android/app/build.gradle.kts` |
| Gradle / AGP | wrapper | `gradle-wrapper.properties` / `gen/android/build.gradle.kts` |

`Cargo.lock` and `package-lock.json` are committed, so dependency versions are
exact. Use `npm ci`, not `npm install`, when verifying.

## What breaks determinism, and what we do about it

- **Absolute paths.** rustc embeds source paths (panic messages, debug refs).
  Everyone's checkout and cargo home live somewhere else, so both get remapped
  to fixed names with `--remap-path-prefix`.
- **PE timestamp (Windows).** The MSVC linker stamps link time into the header.
  `-Clink-arg=/Brepro` replaces it with a hash of the binary - same input, same
  "timestamp".
- **APK signature.** The signing key stays with the maintainer, so your build
  can't be byte-identical to the *signed* APK. The standard answer (same as
  F-Droid's): build unsigned, then let `apksigcopier` graft the release
  signature onto your build and compare the rest byte for byte.

The release workflow (`.github/workflows/release.yml`) exports the same flags,
so CI artifacts are built exactly like the recipe below.

## Recipe: Windows

From a `x64` shell with rustup and Node 22 (`REPO` = your checkout path):

```powershell
$env:RUSTFLAGS = "--remap-path-prefix=$REPO=/build --remap-path-prefix=$env:USERPROFILE\.cargo=/cargo -Clink-arg=/Brepro"
cd $REPO\app
npm ci
npm run build          # frontend -> app/dist
cd $REPO
cargo build --release -p svitok-app
Get-FileHash target\release\svitok-app.exe -Algorithm SHA256
```

Compare against `Svitok-VERSION-windows.exe` (the portable executable) in the
release and its line in `SHA256SUMS.txt`. The NSIS installer wraps this same
executable, but the installer itself is not byte-reproducible yet (NSIS packs
file timestamps); verify the portable exe.

## Recipe: Android

With JDK 21 (Temurin), the Android SDK, NDK 28.2.13676358 and the Rust Android
targets installed (`rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android`):

```bash
export RUSTFLAGS="--remap-path-prefix=$REPO=/build --remap-path-prefix=$HOME/.cargo=/cargo"
export JAVA_HOME=...   # Temurin 21
export ANDROID_HOME=... NDK_HOME=$ANDROID_HOME/ndk/28.2.13676358
cd $REPO/app
npm ci
npx tauri android build --apk
```

Without the maintainer's `keystore.properties` this produces
`app-universal-release-unsigned.apk`. Then:

```bash
pip install apksigcopier
apksigcopier compare Svitok-VERSION-android.apk --unsigned app-universal-release-unsigned.apk
```

Silence means the APKs are identical except for the signature - i.e. the
released APK was built from this source.

`compare` shells out to `apksigner`; on Windows the `.bat` shim is sometimes
not found even on PATH. Equivalent check without it: graft the release
signature onto your build and compare hashes yourself -

```powershell
apksigcopier copy Svitok-VERSION-android.apk app-universal-release-unsigned.apk grafted.apk
Get-FileHash Svitok-VERSION-android.apk, grafted.apk -Algorithm SHA256
```

Matching hashes prove the same thing, byte for byte.

The released APK is built with exactly this recipe (same `RUSTFLAGS`, same
pinned toolchains) plus the signing key.

## Verifying a release

1. Check out the release tag: `git checkout vX.Y.Z`.
2. Follow a recipe above.
3. Compare hashes with `SHA256SUMS.txt` from the release (desktop), or run
   `apksigcopier compare` (Android).

If your bytes differ, please open an issue with your toolchain versions and the
diff - either the recipe is missing a knob, or something is genuinely wrong,
and both are worth knowing.
