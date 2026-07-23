# Autofill

Svitok can fill logins and passwords for you - on Android as a system autofill
service, on desktop through a browser extension. In both cases the password is
never stored: the app derives one on request and hands back a single result. The
seed and phrase never leave the app.

## Android

Svitok registers as a system autofill service. Once you pick it, it offers to
fill login forms in apps and browsers.

**Turn it on:**

1. Android Settings -> search for "Autofill service" (often under System ->
   Languages & input -> Autofill service, or Passwords & security).
2. Choose **Svitok**.

**Chrome note (Android 14+):** Chrome uses its own password manager by default.
To let Svitok fill in Chrome, open Chrome -> Settings -> **Autofill services** ->
turn on **Autofill using another service**. Without it Chrome won't hand its
fields to any third-party manager. Firefox and most apps work without this step.

**How a fill goes:**

1. Focus a login or password field. A "Svitok - <site>" suggestion appears.
2. Tap it. Svitok asks for your fingerprint (the seed is sealed in the Keystore),
   then your master phrase.
3. It derives the password for that site and fills the fields.

Matching is by registrable domain, so a saved `github.com` also matches
`gist.github.com`. For this to work the entry has to be saved as a domain
(`github.com`), not a free-form name.

An entry also matches on its extra domains ("other domains" in the edit form) -
one service living on several domains fills the same password on all of them.
Several accounts on one domain are separate entries told apart by login; the
suggestion list shows each as "site (login)".

The fill dialog is screen-capture protected, and the derivation runs the full
memory-hard KDF, so expect a second or two after you enter the phrase.

## Desktop (browser extension)

On desktop a small browser extension talks to the running Svitok app over a
local socket. The app matches the page's domain, and if it's locked it comes to
the front, asks for your phrase, and then fills - no second click.

```
page  <->  extension  <->  svitok-host  <->  Svitok app (local socket)
```

`svitok-host` is a thin relay with no secrets; the app does the matching and
derivation. The channel is scoped to your user account, and the extension
authenticates with a pairing token you copy from the app once.

**Set it up (manual, until the installer does it for you):**

1. Build the app and the host, or use a release build:
   ```
   cargo build --release -p svitok-host
   npm --prefix app run tauri build
   ```
   The host is at `target/release/svitok-host` (`.exe` on Windows).

2. Load the extension: your browser -> Extensions -> enable Developer mode ->
   **Load unpacked** -> pick the `extension/` folder. Copy the extension ID.

3. Register the native host. Edit `extension/native-host/app.svitok.host.json`:
   set `path` to the absolute path of `svitok-host`, and put your extension ID in
   the `chrome-extension://<ID>/` origin. Then place the file where the browser
   looks:
   - **Windows:** put the JSON anywhere, then point a registry value at it -
     `HKCU\Software\Google\Chrome\NativeMessagingHosts\app.svitok.host` (Default) =
     full path to the JSON.
   - **macOS:** `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/app.svitok.host.json`
   - **Linux:** `~/.config/google-chrome/NativeMessagingHosts/app.svitok.host.json`

   Restart the browser so it picks up the new host.

4. Pair: open Svitok, unlock it, go to **Settings -> Browser extension**, copy the
   token. Open the extension's options and paste it.

**How a fill goes:** focus a login field, pick the Svitok suggestion. If the app
is locked it raises its window and asks for the phrase; once you type it, the
fields fill on their own. The suggestion draws above the field so it doesn't sit
under the browser's own password dropdown; you can also turn the browser's built
in password manager off to keep things clean.

**Note:** matching and derivation only work while the app is running. Locked is
fine - it unlocks on demand. Closed is not.
