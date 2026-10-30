# nocast

A tiny app launcher and window manager for macOS. One small background app, no Dock icon, no settings window, one text file for config.

## Install / update

```sh
curl -fsSL https://raw.githubusercontent.com/cucumbers2blue/nocast/main/install.sh | sh
```

Then:

1. Allow Accessibility when macOS asks (System Settings > Privacy & Security > Accessibility > nocast on). You'll be asked again after each update. Websites also ask once to control Safari (or Chrome).
2. If you use `cmd+space`, turn off Spotlight's shortcut in System Settings > Keyboard > Keyboard Shortcuts > Spotlight.

Uninstall:

```sh
curl -fsSL https://raw.githubusercontent.com/cucumbers2blue/nocast/main/install.sh | sh -s uninstall
```

## Config

`~/.config/nocast.conf` is created on first run. Changes apply as soon as you save.

```
launcher   cmd+space

Safari      opt+j
Notes       opt+k
Reminders   opt+l
Logic Pro

Gmail      https://mail.google.com   opt+g
GitHub     https://github.com

full    opt+up
left    opt+left
right   opt+right
next    opt+cmd+up
```

- Apps: the app name (or a full path to a .app), optionally followed by a hotkey. Without a hotkey, the app is only found by searching.
- Modifiers: `cmd`, `opt`, `ctrl`, `shift`. Keys: `a`–`z`, `0`–`9`, `space`, `up`, `down`, `left`, `right`, `return`, `tab`, `escape`, `f1`–`f12`, punctuation.
- An app hotkey opens the app, or hides it if it's already in front.
- Websites: a name, the address, and an optional hotkey. If a Safari tab already has the site open (any page under that address), nocast switches to it; press again to cycle through matching tabs. Otherwise it opens a new tab. The first time, macOS asks to let nocast control Safari.
- Browser: add a line `browser chrome` to use Google Chrome for websites instead. `browser safari` is the default.
- Launcher: type, and the best match completes in grey. Its assigned shortcut appears on the right using macOS symbols (for example, `⌥J` or `⌥⌘G`), and updates as you cycle matches. Return opens it, up/down or tab cycles matches, escape closes.
- `full` fills the screen except the menu bar and Dock. `next` moves the window to the next display, keeping its relative size and position.

Logs: `~/Library/Logs/nocast.log` (errors only). Add a line `debug` to the config to log every step, and watch it live with `open -a Console ~/Library/Logs/nocast.log`.

## How it works

About 800 lines of Rust in `src/`. A keyboard event tap (on its own thread) catches the configured hotkeys and swallows them; everything else passes through untouched. Window moves use the Accessibility API. GitHub Actions builds and signs (ad-hoc) on each push to `main` and publishes a release.
