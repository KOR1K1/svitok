# Roadmap

A rough plan, not a promise. Priorities move around, and a good PR beats a line on this list. If you want to pick something up, open an issue first so we don't both build it two different ways.

## Next up

The things most likely to happen first.

- **Reproducible builds.** The README already calls this the highest-value trust improvement, and it is. "Build it yourself and get the exact same binary that's in the release" is the difference between trusting the code and trusting me. Also a prerequisite for doing F-Droid properly.
- **Import from other managers.** A CSV / Bitwarden / KeePass export turned into your site list. Without it, a new user types everything in by hand, which nobody wants to do.

## Shipped

- **Autofill** (v0.2.0) - a system autofill service on Android, a browser extension plus native host on desktop. Passwords are derived per request and never stored.
- **Multiple accounts per domain and domain aliases** (v0.3.0) - entries differ by login, one service on several domains matches everywhere; all matching metadata stays out of derivation.
- Clipboard hardening (sensitive flag on Android, out of history on Windows), forced screen-capture protection on secret screens, `mlock`/`VirtualLock` for key material.

## Bigger things

- **macOS, Linux, and Windows binaries** now come from a tag-triggered release workflow (`.github/workflows/release.yml`); Android is built and signed locally. Making those builds reproducible - the same bytes from the same source - is the separate line above.
- **An outside look at the crypto.** It's hand-rolled and hasn't had a real audit - that's stated plainly in the README, and it should get fixed by someone who isn't me. Community review, or one of the free audit programs.

## Security odds and ends

Smaller hardening that's already noted in the code and the audit:

- On Linux, tell "keyring is unavailable" apart from "no seed here" so restore doesn't dead-end.

## F-Droid

I want it there. It's also the hardest item, for two reasons.

First, the QR scanner uses Google's ML Kit, which is proprietary and tied to Play Services. F-Droid won't accept that. Two ways out: ship an F-Droid flavor with the camera scanner removed (sync still works through backup / paste), or swap ML Kit for a free decoder like ZXing so the camera keeps working everywhere.

Second, F-Droid builds from source on their own servers, and a Tauri Android build - Rust cross-compile, NDK, npm, gradle - is not a trivial recipe to get green there.

Once those are handled: fastlane metadata in the repo (descriptions, per-version changelogs, screenshots), a merge request to `fdroiddata` with the build recipe, and `UpdateCheckMode: Tags` so a new git tag gets built and published on its own. After that, releasing is just tagging.

If reach matters before all that lands, IzzyOnDroid takes prebuilt APKs with much lighter requirements.

## Ideas, maybe

Not committed to any of these, just things worth thinking about:

- Folders, tags, or favorites for the site list, and a better search.
- Offline icons for sites, and issuer icons on TOTP entries.
- Windows Hello / Touch ID for a quick desktop unlock instead of the phrase.
- A light theme.
- A duress phrase that opens a decoy vault. Paranoid, but on-brand.
- An optional "change the phrase without changing any passwords" mode (a wrapped master key). Deliberately not the default - see below.
- More languages. The i18n is two flat dictionaries; adding one is copy, translate, done.

## Not planned, on purpose

Some things are missing by design, not by accident:

- **No cloud sync, no account, no server.** That's the whole point - there's nothing to breach. Your list moves by QR or a text backup you control.
- **No built-in auto-update over the network.** An offline app phoning home would defeat the idea. Updates come from GitHub, F-Droid, or a manual download.
- **The derivation scheme is frozen.** The golden vectors pin it. Paper written today has to keep working years from now, so the algorithm doesn't get to change.
