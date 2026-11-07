# NotiFansly Desktop

Cross-platform (Linux, macOS, Windows) GPUI desktop app that links a Fansly
creator account to NotiFansly using **management link tokens** — no passwords,
no API keys. After a one-time connect, all sync (subscription tiers,
followers, subscribers) runs **server-side**; the app is a thin management and
status client that lives in your system tray.

![status: v0.1](https://img.shields.io/badge/version-0.1-blue)

## Features

- **First-launch onboarding** — a 3-step wizard for non-technical users:
  welcome → how to create a Fansly management link (with an "Open Fansly"
  shortcut) → paste it. Each step fades in; progress dots show where you
  are; it never repeats after completion.
- **Connect via Fansly management link** — paste the management session link
  from your Fansly creator dashboard, press Connect. The (single-use) link is
  claimed by the API and the session is stored server-side, encrypted at rest.
- **Connected success state** — after connecting (or recovering), a
  full-screen "You're connected!" step tells users they can simply close the
  window; sync continues from the system tray on its own.
- **Account recovery** — reinstalling or moving devices? Paste a fresh
  management link: your existing account and sync key are found and restored.
- **System tray** — close the window and the app keeps running in the tray
  (Show / Sync now / Quit). Sync is server-side, so a parked window is fine.
- **Optional launch at login** — one click in the dashboard footer (XDG
  autostart on Linux, LaunchAgent on macOS, Run registry key on Windows),
  pointing at wherever the binary currently lives.
- **Status at a glance** — management-token health, last server sync time,
  Discord linking, and a masked sync key with copy/reveal.
- **Light / dark mode** — toggle in the header, persisted per device. The
  palette follows the yap-a-thon dock theme (surface cards, status pills,
  named text sizes, brand-blue #2699f6 accent).
- **No secrets in the binary** — the app talks to the public desktop
  endpoints of the sync API (/api/v1/desktop/*); no shared server secrets.

## Endpoints used (Go sync API)

| Endpoint | Auth | Purpose |
| --- | --- | --- |
| `POST /api/v1/desktop/connect` | single-use Fansly management claim link | link + recover (returns your sync key) |
| `POST /api/v1/desktop/status` | sync key | account/token/last-sync status |
| `POST /api/v1/desktop/trigger-sync` | sync key | kick off a server-side sync |

## Configuration

State lives in `<config dir>/notifansly-desktop/state.json` (0600): the
connected account, its sync key, and the API server URL (default
`https://api.notifansly.xyz`, editable in the UI footer).

Environment override: `NOTIFANSLY_API_URL` is not used by this app — the API
URL is edited in the UI and persisted per-device.

## Building

```sh
cargo build            # debug
cargo build --release  # cargo-release profile is stripped + LTO
```

Linux tray dependencies (Arch, adjust for your distro):
`gtk3 libappindicator-gtk3` (or `libayatana-appindicator`).

## Notes

- Closing the window **minimizes** it (GPUI has no true hide-to-tray API in
  this release); use tray → Quit to exit.
- Old AccountManager (Tauri) app users: your Fansly auth-token flow still
  works against `/api/v1/sync`, but this app replaces it with management
  links + server-side sync.

## Demo mode

```sh
cargo run -- --demo      # or: NOTIFANSLY_DEMO=1
```

Canned data (`@aureliaf`, active token, linked Discord) with no network:
connect accepts any text, status/sync are simulated, and the whole flow
(onboarding -> success -> dashboard) can be previewed and screenshotted
without touching a server. Delete `~/.config/notifansly-desktop/state.json`
to start over.
