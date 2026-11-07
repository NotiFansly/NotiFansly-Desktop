# NotiFansly Desktop

The NotiFansly desktop app for Fansly creators. Connect your account once
and your Fansly tiers, followers and subscribers sync to NotiFansly
automatically — you don't need to keep anything open.

## Download

Grab the one for your computer from the
[Releases page](https://github.com/NotiFansly/NotiFansly-Desktop/releases):

| Your computer | File to download |
| --- | --- |
| Windows | `notifansly-desktop-windows-x86_64.zip` |
| macOS (Apple Silicon) | `notifansly-desktop-macos-aarch64.zip` |
| Linux | `notifansly-desktop-linux-x86_64.zip` |

Unzip it and run the app inside. On macOS, the first time you open it,
right-click it and choose **Open** (once), then it opens normally after that.

## First launch

The app walks you through it — three screens:

1. **Welcome** — a quick intro.
2. **Create your management link** — you'll open your Fansly account's
   Management Sessions page and click **Create Session**. You can leave every
   permission unchecked (unchecking all of them is correct — syncing doesn't
   need any). The app has a button that opens the right Fansly page for you.
3. **Paste your link** — paste the link Fansly gave you and press **Connect**.

That's it. Your account is now connected and syncing happens on NotiFansly's
servers, all by itself.

> Moved to a new computer, or reinstalled? Just create a fresh management
> link in Fansly and paste it — the app finds your existing account and
> restores your sync key automatically.

## The system tray (the icon near your clock)

Closing the window **doesn't quit the app** — it keeps running quietly in
the tray so your sync never stops. To actually quit, use the tray icon's
menu → **Quit**.

| Tray menu item | What it does |
| --- | --- |
| Show NotiFansly | Brings the window back |
| Sync now | Asks the server to sync immediately |
| Quit | Fully closes the app |

The icon's colour is also a status signal:

| Icon | Meaning |
| --- | --- |
| Normal colors | Everything is working |
| Red | Your management link has expired or been revoked — open the app and reconnect with a fresh one |
| Greyed out | Not connected yet (or the app can't reach the server) |

## Sync key (what it is, where to use it)

Your **sync key** is shown on the main screen, with a Copy button. It does
two things:

- Paste it into the [NotiFansly web dashboard](https://notifansly.xyz) under
  **Settings** to link your Discord account.
- It restores your account if you ever reinstall the app — save a copy
  somewhere safe and don't share it with anyone.

## Options

- **Launch at login** — a button on the main screen, off by default.
- **Light / dark mode** — the moon/sun button in the top-right corner.

## Where your data lives

The app keeps one small file with your connected account and sync key:

| System | Location |
| --- | --- |
| Windows | `C:\Users\<you>\AppData\Roaming\notifansly-desktop\state.json` |
| macOS | `~/Library/Application Support/notifansly-desktop/state.json` |
| Linux | `~/.config/notifansly-desktop/state.json` |

Nothing else is stored on your computer, and the file is only readable by
your user account. Want a fresh start? Quit the app and delete that file —
you'll get the first-launch wizard again.

## If something looks wrong

- **"Token invalid/expired"** — Fansly management links don't last forever.
  Go back to Fansly, create a new management link, and connect it again
  (same as the first time — your account and key are found automatically).
- **A sync won't trigger** — check your internet connection, then press
  **Refresh**.
- Anything else: reach out on the NotiFansly Discord.

---

*Developers: build instructions and internals live in
[DEVELOPMENT.md](DEVELOPMENT.md).*
