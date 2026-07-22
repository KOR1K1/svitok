# Svitok browser extension

Autofills logins and passwords from the Svitok desktop app. Passwords are not
stored - the app derives one on request and the extension only fills it.

How it fits together:

```
page  <->  content.js  <->  background.js  <->  native host (svitok-host)  <->  Svitok app (local socket)
```

The extension talks to `svitok-host` over Chrome native messaging. The host is a
thin relay that forwards requests to the running Svitok app over a local socket
(named pipe on Windows, unix socket on macOS/Linux), scoped to your user. The app
matches the page's domain against your site list and, if it's unlocked, derives
one password. The key and phrase never leave the app.

## Install (development)

1. Build the app and the host:
   ```
   cargo build --release -p svitok-host
   cargo build --release -p svitok-app   # or the normal desktop app build
   ```
   The host binary is at `target/release/svitok-host` (`.exe` on Windows).

2. Load the extension: Chrome -> Extensions -> enable Developer mode ->
   "Load unpacked" -> select this `extension/` folder. Copy the extension ID
   Chrome shows.

3. Register the native host. Take `native-host/app.svitok.host.json`, set:
   - `path` to the absolute path of the `svitok-host` binary,
   - the `chrome-extension://<ID>/` origin to your extension ID.

   Then place it where Chrome looks for it:
   - Windows: put the file anywhere, then create a registry value pointing to it:
     `HKCU\Software\Google\Chrome\NativeMessagingHosts\app.svitok.host` (Default) =
     full path to the json.
   - macOS: `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/app.svitok.host.json`
   - Linux: `~/.config/google-chrome/NativeMessagingHosts/app.svitok.host.json`

4. Pair: open the Svitok app, unlock it, go to Settings -> Browser extension,
   copy the pairing token. Open the extension's options and paste it.

5. Open a site's login page, focus the login or password field, and pick the
   Svitok suggestion. Unlock the app if it asks.

## Notes

- The app must be running and unlocked to fill. Locked -> the extension shows
  "unlock Svitok".
- Matching is by registrable domain (same rule as the app), so a saved
  `github.com` matches `gist.github.com`.
