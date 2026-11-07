# Development notes

Everything here is for people working *on* the app, not using it — the
user-facing page is the [README](README.md).

## Stack

GPUI 0.2.2 (crates.io) + tray-icon 0.21, same stack as an earlier internal GPUI codebase.
The Go sync API it talks to lives in `../notifansly-api`
(`/api/v1/desktop/*` endpoints, added for this app).

## Local development

```sh
cargo run                 # debug run
cargo run -- --demo       # demo mode (see below)
cargo build --release     # release/profile: stripped + LTO
```

Linux build dependencies: `libgtk-3-dev libayatana-appindicator3-dev`
(tray) plus GPUI's X11/Wayland/fontconfig packages — the full apt list is
in `.github/workflows/release.yml`.

## Demo mode (`--demo` or `NOTIFANSLY_DEMO=1`)

Canned data with no network: connect accepts **any non-empty text**
(`test` is fine), status shows fake creator `@aureliaf`, and sync now
simulates a ~6s server sync. The periodic status poll is disabled.
Useful for previewing and screenshotting the whole flow (wizard →
success → dashboard) without touching a server.

`NOTIFANSLY_ONBOARD_STEP=<0|1|2>` starts directly on that wizard step
(only sensible with an empty state file). Delete
`~/.config/notifansly-desktop/state.json` to reset.

## State file

`<config dir>/notifansly-desktop/state.json` (0600 on Unix): API URL,
connected account, sync key, appearance, onboarding-completed flag.
Written temp-then-rename; see `src/state.rs`.

## Windows / macOS notes

- **A Windows *release* binary cannot be cross-compiled from Linux.**
  gpui 0.2.2's build script pre-compiles its DirectX shaders with `fxc.exe`
  from the Windows SDK (release profile only — build scripts keep
  `debug_assertions` on, so shader building is silently skipped and the
  lib then fails on the missing `include!`). Build Windows binaries on the
  CI runner (MSVC + SDK), which is what the workflow does.
- `cargo check --target x86_64-pc-windows-gnu` is the local smoke test
  for the Windows code path (it has caught real bugs: `MSG` has no
  `Default`, `HWND` is `isize`, `read_from_primary` is Linux-only).
- macOS tray is created on the main thread inside GPUI's run callback
  (tray-icon requires the app's own event loop; GPUI owns the NSApp loop).
- `Cargo.lock` pins `libc =0.2.188` — libc 0.2.190+ removed the `ENOATTR`
  alias that `xattr 0.2` (transitive via gpui's zed-async-tar) references.

## Releases

`.github/workflows/release.yml`: pushing a `vX.Y.Z` tag (or a manual
workflow dispatch) builds all three platforms and attaches the zips to
the GitHub release. Loose bundles are always uploaded as CI artifacts,
including non-tag dispatch runs.
