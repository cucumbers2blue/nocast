# AGENTS.md

nocast is a tiny macOS app launcher and window manager in Rust (edition 2024): a background app with no Dock icon and no settings window, configured by one text file, `~/.config/nocast.conf`.

## Layout

- `src/main.rs`: startup, Accessibility prompt, keyboard event tap thread, config hot reload, dispatch of actions to the main thread, opening apps.
- `src/config.rs`: parses `nocast.conf` (apps, websites, hotkeys, window actions, `debug`), the default config, app lookup; unit tests.
- `src/launcher.rs`: the floating search panel, match ranking and grey completion; unit tests.
- `src/windows.rs`: `full`/`left`/`right`/`next` window moves via the Accessibility API; unit tests.
- `src/safari.rs`: website hotkeys; finds or cycles matching Safari tabs via AppleScript, else opens a new tab; unit tests.
- `src/ffi.rs`: the C APIs objc2 doesn't cover (CGEventTap, AXUIElement, CoreFoundation, libdispatch, Apple Events permission).
- `src/log.rs`: `log!` (always) and `debug!` (only with `debug` in the config) to stderr, which the LaunchAgent sends to `~/Library/Logs/nocast.log`.
- `install.sh`: install/update/uninstall; downloads the latest release into `~/Applications/nocast.app`, sets up the LaunchAgent, resets stale permissions.
- `bundle/Info.plist`: app bundle template (`__VERSION__` is filled in by CI).
- `.github/workflows/release.yml`: CI release pipeline.

## Build and test (macOS only)

- `cargo build`, `cargo test`, `cargo build --release` (size-optimized profile: `opt-level = "s"`, LTO, `panic = "abort"`, stripped).
- CI runs on every push to `main` that touches `src/`, `Cargo.*`, `bundle/`, `install.sh` or the workflows: `cargo test`, release build, bundle, ad-hoc codesign, then publishes a GitHub release. Merging to `main` ships a release to every user who updates.

## Platform notes

- Hotkeys and window moves need the Accessibility permission; each new build loses it and must be granted again.
- Website hotkeys automate Safari via Apple Events; macOS asks once.
- Tests must pass on case-insensitive filesystems (the macOS default).

## Style

- Keep it tiny and native-feeling. Use as few dependencies as possible (today: objc2, objc2-app-kit, objc2-foundation, block2).
- No analytics, tracking or network calls beyond what a feature needs.
- One plain-text config file, no settings UI.
- Log errors only, unless `debug` is set.
- Concise, technical copy in code, logs and docs.
- Make small changes in small steps.
- When behavior or config syntax changes, update README.md in the same PR.

## Workflow

- Work on a branch and open a PR for Peter to review. Never push to `main` or merge.
- Never commit secrets.
