# Privacy Policy

**Short version: Svitok collects nothing, sends nothing, and has no way to.**

Last updated: 2026. Applies to the Svitok app (Android and desktop).

## No data leaves your device

Svitok has no servers, no accounts, no analytics, no ads, no crash reporting, and no third-party SDKs. The Android release build ships **without the `INTERNET` permission**, so it physically cannot make a network connection. The desktop build talks to nothing but the operating system.

There is no telemetry to turn off, because there is none.

## What's stored, and where (all local)

Everything stays on the device you're using:

- **Seed** - your 128-bit secret. On Android it's encrypted with a key held in the Android Keystore, unlocked by your biometrics or device credential; the key never leaves the hardware-backed store. On desktop it's kept in the OS secret store (Windows Credential Manager, macOS Keychain, or Linux Secret Service). It is never written to disk in the clear and never sent anywhere.
- **Site list** (`sites.txt`) - the names, logins, counters and character policies of your entries. This is metadata, not passwords; the passwords themselves are never stored - they're recomputed when you ask for one.
- **Vault** (`vault.b32`) - an encrypted blob for TOTP secrets, recovery codes, notes and foreign passwords. It's ciphertext; without your seed and phrase it's useless.
- **Interface settings** - language, auto-lock timeout, haptics on/off, and similar, in local storage. No secrets here.

Your **master phrase** is never stored anywhere at all. It exists only in your head and, briefly, in memory while a password is being derived - after which the working key material is wiped.

## Permissions

- **Camera** (Android, optional) - used only to scan a QR code when you sync your site list or import a 2FA secret. Frames are processed on-device and never uploaded. The permission is only requested when you actually use the scanner.

That's the only sensitive permission the app asks for.

## Clipboard

When you copy a password, it goes to the system clipboard so you can paste it. On Android the copy is marked sensitive (hidden from the clipboard preview and kept out of keyboard cloud-sync where the OS honors it), and Svitok clears it automatically after a short timer and when the app locks. Once it's in the clipboard, though, other apps on the device with clipboard access can read it - that's an OS-level reality, not something any app fully controls.

## Backups you create

If you use the backup feature, Svitok hands *you* a block of text (your site list plus the encrypted vault) and it's entirely up to you where it goes. It contains no passwords and no seed, but it does reveal *which* sites you have accounts on - treat that as you would any personal metadata. Svitok never uploads it anywhere; that's your choice to make.

## Deleting your data

"Destroy Svitok" in Settings erases the seed from the secure store and deletes the local files. On desktop you can also remove the app's data directory; on Android, uninstalling the app removes everything. Because passwords are derived and never stored, there's nothing left behind to recover once the seed is gone - which is exactly why your paper copy matters.

## Children

Svitok isn't directed at children and collects no data from anyone, of any age.

## Changes

If this policy ever changes, the update will land in this file in the public repository, with the date above.

## Contact

Questions or concerns: open an issue at <https://github.com/KOR1K1/svitok> (or a private security advisory for anything sensitive).
