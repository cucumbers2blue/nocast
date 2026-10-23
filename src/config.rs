// Config file: ~/.config/nocast.conf. One setting per line, `#` starts a comment.
//
//   launcher  cmd+space
//   Safari    opt+j          (an app, with an optional hotkey)
//   Logic Pro                (an app you only search for)
//   full      opt+up         (also: left, right, next)

use std::path::PathBuf;

pub const DEFAULT: &str = "\
# nocast config. Saved changes apply right away.
# Apps: the app name, optionally followed by a hotkey.
# Modifiers: cmd, opt, ctrl, shift. Keys: a-z, 0-9, space, up, down, left, right,
# return, tab, escape, f1-f12, and - = [ ] ; ' , . / \\ `

launcher   cmd+space

Safari      opt+j
Notes       opt+k
Reminders   opt+l
Finder

full    opt+up
left    opt+left
right   opt+right
next    opt+cmd+up
";

pub const CMD: u64 = 0x100000;
pub const OPT: u64 = 0x80000;
pub const CTRL: u64 = 0x40000;
pub const SHIFT: u64 = 0x20000;
pub const MODS: u64 = CMD | OPT | CTRL | SHIFT;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hotkey {
    pub key: u16,
    pub mods: u64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Win {
    Full,
    Left,
    Right,
    Next,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Launcher,
    App(usize),
    Win(Win),
}

pub struct App {
    pub name: String,
    pub hotkey: Option<Hotkey>,
    pub path: Option<PathBuf>,
}

#[derive(Default)]
pub struct Config {
    pub launcher: Option<Hotkey>,
    pub apps: Vec<App>,
    pub win: Vec<(Hotkey, Win)>,
}

impl Config {
    pub fn lookup(&self, key: u16, mods: u64) -> Option<Action> {
        let hk = Hotkey { key, mods };
        if self.launcher == Some(hk) {
            return Some(Action::Launcher);
        }
        if let Some(i) = self.apps.iter().position(|a| a.hotkey == Some(hk)) {
            return Some(Action::App(i));
        }
        self.win.iter().find(|(h, _)| *h == hk).map(|(_, w)| Action::Win(*w))
    }

    pub fn parse(text: &str) -> Config {
        let mut cfg = Config::default();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            // Settings: a reserved word followed by a hotkey.
            let (first, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let setting = match first.to_lowercase().as_str() {
                "launcher" => Some(None),
                "full" => Some(Some(Win::Full)),
                "left" => Some(Some(Win::Left)),
                "right" => Some(Some(Win::Right)),
                "next" => Some(Some(Win::Next)),
                _ => None,
            };
            if let (Some(what), Some(h)) = (setting, parse_hotkey(rest)) {
                match what {
                    None => cfg.launcher = Some(h),
                    Some(w) => cfg.win.push((h, w)),
                }
                continue;
            }

            // Anything else is an app: its name, plus an optional hotkey (a word with '+').
            let mut words: Vec<&str> = line.split_whitespace().collect();
            if words.len() > 2 && words[0] == "app" {
                words.drain(..2); // old format: "app keyword [hotkey] Name"
            }
            let mut hotkey = None;
            if let Some(i) = words.iter().position(|w| w.contains('+')) {
                hotkey = parse_hotkey(words[i]);
                if hotkey.is_none() {
                    eprintln!("nocast: line {}: bad hotkey: {raw}", n + 1);
                }
                words.remove(i);
            }
            let name = words.join(" ");
            if name.is_empty() {
                continue;
            }
            let path = find_app(&name);
            if path.is_none() {
                eprintln!("nocast: app not found: {name}");
            }
            let display = name.rsplit('/').next().unwrap_or(&name).trim_end_matches(".app");
            cfg.apps.push(App { name: display.to_string(), hotkey, path });
        }
        cfg
    }
}

/// Accepts "opt+up", "opt + arrow up", "option+left arrow", "cmd+opt+up", ...
pub fn parse_hotkey(s: &str) -> Option<Hotkey> {
    let s = s.to_lowercase().replace("arrow", "");
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut mods = 0;
    let mut key = None;
    for part in s.split('+').filter(|p| !p.is_empty()) {
        match part {
            "cmd" | "command" | "⌘" => mods |= CMD,
            "opt" | "option" | "alt" | "⌥" => mods |= OPT,
            "ctrl" | "control" | "⌃" => mods |= CTRL,
            "shift" | "⇧" => mods |= SHIFT,
            k => key = Some(keycode(k)?),
        }
    }
    // Every hotkey needs a modifier, otherwise it would eat normal typing.
    if mods == 0 {
        return None;
    }
    Some(Hotkey { key: key?, mods })
}

/// macOS virtual key codes (US layout key positions).
pub fn keycode(k: &str) -> Option<u16> {
    Some(match k {
        "a" => 0, "s" => 1, "d" => 2, "f" => 3, "h" => 4, "g" => 5, "z" => 6, "x" => 7,
        "c" => 8, "v" => 9, "b" => 11, "q" => 12, "w" => 13, "e" => 14, "r" => 15,
        "y" => 16, "t" => 17, "1" => 18, "2" => 19, "3" => 20, "4" => 21, "6" => 22,
        "5" => 23, "=" => 24, "9" => 25, "7" => 26, "-" => 27, "8" => 28, "0" => 29,
        "]" => 30, "o" => 31, "u" => 32, "[" => 33, "i" => 34, "p" => 35,
        "return" | "enter" => 36, "l" => 37, "j" => 38, "'" => 39, "k" => 40, ";" => 41,
        "\\" => 42, "," => 43, "/" => 44, "n" => 45, "m" => 46, "." => 47,
        "tab" => 48, "space" => 49, "`" => 50, "delete" | "backspace" => 51,
        "escape" | "esc" => 53,
        "f1" => 122, "f2" => 120, "f3" => 99, "f4" => 118, "f5" => 96, "f6" => 97,
        "f7" => 98, "f8" => 100, "f9" => 101, "f10" => 109, "f11" => 103, "f12" => 111,
        "left" => 123, "right" => 124, "down" => 125, "up" => 126,
        _ => return None,
    })
}

/// "Safari" -> /Applications/Safari.app. A full path to a .app also works.
fn find_app(name: &str) -> Option<PathBuf> {
    if name.starts_with('/') || name.starts_with('~') {
        let p = PathBuf::from(name.replacen('~', &home(), 1));
        return p.exists().then_some(p);
    }
    let file = if name.ends_with(".app") { name.to_string() } else { format!("{name}.app") };
    let home_apps = format!("{}/Applications", home());
    [
        "/Applications",
        "/System/Applications",
        "/System/Applications/Utilities",
        "/Applications/Utilities",
        home_apps.as_str(),
        "/System/Library/CoreServices",
    ]
    .iter()
    .map(|d| PathBuf::from(d).join(&file))
    .find(|p| p.exists())
}

pub fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

pub fn path() -> PathBuf {
    PathBuf::from(home()).join(".config/nocast.conf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys() {
        assert_eq!(parse_hotkey("opt+j"), Some(Hotkey { key: 38, mods: OPT }));
        assert_eq!(parse_hotkey("command+space"), Some(Hotkey { key: 49, mods: CMD }));
        assert_eq!(parse_hotkey("opt+arrow up"), Some(Hotkey { key: 126, mods: OPT }));
        assert_eq!(parse_hotkey("opt+left arrow"), Some(Hotkey { key: 123, mods: OPT }));
        assert_eq!(parse_hotkey("opt+command+up"), Some(Hotkey { key: 126, mods: OPT | CMD }));
        assert_eq!(parse_hotkey("j"), None);
        assert_eq!(parse_hotkey("opt+nope"), None);
    }

    #[test]
    fn default_config() {
        let c = Config::parse(DEFAULT);
        assert_eq!(c.launcher, Some(Hotkey { key: 49, mods: CMD }));
        let names: Vec<&str> = c.apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Safari", "Notes", "Reminders", "Finder"]);
        assert_eq!(c.apps[3].hotkey, None);
        assert_eq!(c.win.len(), 4);
        assert!(matches!(c.lookup(126, OPT | CMD), Some(Action::Win(Win::Next))));
        assert!(matches!(c.lookup(38, OPT), Some(Action::App(0))));
        assert!(c.lookup(38, CMD).is_none());
    }

    #[test]
    fn app_lines() {
        let c = Config::parse("Logic Pro\nLogic Pro  opt+p\nopt+s Safari\napp saf opt+j Safari\napp notes Notes\nNext\nfull    opt+arrow up\nleft opt+left arrow\n");
        let apps: Vec<(&str, Option<Hotkey>)> = c.apps.iter().map(|a| (a.name.as_str(), a.hotkey)).collect();
        assert_eq!(apps, [
            ("Logic Pro", None),
            ("Logic Pro", Some(Hotkey { key: 35, mods: OPT })),
            ("Safari", Some(Hotkey { key: 1, mods: OPT })),
            ("Safari", Some(Hotkey { key: 38, mods: OPT })),
            ("Notes", None),
            ("Next", None),
        ]);
        assert_eq!(c.win, vec![
            (Hotkey { key: 126, mods: OPT }, Win::Full),
            (Hotkey { key: 123, mods: OPT }, Win::Left),
        ]);
    }
}
