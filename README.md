# nocast

A tiny app launcher and window manager for macOS. One small background app, no Dock icon, no settings window, one text file for config.

## Install / update

```sh
curl -fsSL https://raw.githubusercontent.com/cucumbers2blue/nocast/main/install.sh | sh
```

Then:

1. Allow Accessibility when macOS asks (System Settings > Privacy & Security > Accessibility > nocast on). You'll be asked again after each update.
2. If you use `cmd+space`, turn off Spotlight's shortcut in System Settings > Keyboard > Keyboard Shortcuts > Spotlight.

Uninstall:

```sh
curl -fsSL https://raw.githubusercontent.com/cucumbers2blue/nocast/main/install.sh | sh -s uninstall
```

## Config

`~/.config/nocast.conf` is created on first run. Changes apply as soon as you save.

```
# launcher
launcher   cmd+space

# apps: keyword, optional direct hotkey, app name (or full path to a .app)
app  saf        opt+j   Safari
app  notes      opt+k   Notes
app  reminders  opt+l   Reminders

# windows
full    opt+up
left    opt+left
right   opt+right
next    opt+cmd+up
```

- Modifiers: `cmd`, `opt`, `ctrl`, `shift`. Keys: `a`–`z`, `0`–`9`, `space`, `up`, `down`, `left`, `right`, `return`, `tab`, `escape`, `f1`–`f12`, punctuation.
- A direct app hotkey opens the app, or hides it if it's already in front.
- Launcher: type to filter, up/down or tab to pick, return to open, escape to close.
- `full` fills the screen except the menu bar and Dock. `next` moves the window to the next display, keeping its relative size and position.

Logs: `~/Library/Logs/nocast.log`.

## How it works

About 700 lines of Rust in `src/`. A keyboard event tap (on its own thread) catches the configured hotkeys and swallows them; everything else passes through untouched. Window moves use the Accessibility API. GitHub Actions builds and signs (ad-hoc) on each push to `main` and publishes a release.
