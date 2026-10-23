// Config file: ~/.config/nocast.conf. One setting per line, `#` starts a comment.
//
//   launcher  cmd+space
//   app  saf  opt+j  Safari          (keyword, optional hotkey, app name or path)
//   full  opt+up                     (also: left, right, next)

use std::path::PathBuf;

pub const DEFAULT: &str = "\
# nocast config. Saved changes apply right away.
# Modifiers: cmd, opt, ctrl, shift. Keys: a-z, 0-9, space, up, down, left, right,
# return, tab, escape, f1-f12, and - = [ ] ; ' , . / \\ `

# launcher
launcher   cmd+space

# apps: keyword, optional direct hotkey, app name
app  saf        opt+j   Safari
app  notes      opt+k   Notes
app  reminders  opt+l   Reminders

# windows
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
    pub keyword: String,
    pub name: String,
    pub hotkey: Option<Hotkey>,
    pub hotkey_label: String,
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
            let (cmd, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let rest = rest.trim();
            let bad = |what: &str| eprintln!("nocast: line {}: {what}: {raw}", n + 1);
            match cmd.to_lowercase().as_str() {
                "launcher" => match parse_hotkey(rest) {
                    Some(h) => cfg.launcher = Some(h),
                    None => bad("bad hotkey"),
                },
                "full" | "left" | "right" | "next" => {
                    let w = match cmd.to_lowercase().as_str() {
                        "full" => Win::Full,
                        "left" => Win::Left,
                        "right" => Win::Right,
                        _ => Win::Next,
                    };
                    match parse_hotkey(rest) {
                        Some(h) => cfg.win.push((h, w)),
                        None => bad("bad hotkey"),
                    }
                }
                "app" => {
                    let mut parts = rest.split_whitespace();
                    let Some(keyword) = parts.next() else { bad("missing keyword"); continue };
                    let mut words: Vec<&str> = parts.collect();
                    let mut hotkey = None;
                    let mut hotkey_label = String::new();
                    if words.first().is_some_and(|w| w.contains('+')) {
                        let w = words.remove(0);
                        hotkey = parse_hotkey(w);
                        match hotkey {
                            Some(h) => hotkey_label = label(h, w),
                            None => bad("bad hotkey"),
                        }
                    }
                    let name = words.join(" ");
                    if name.is_empty() {
                        bad("missing app name");
                        continue;
                    }
                    let path = find_app(&name);
                    if path.is_none() {
                        eprintln!("nocast: app not found: {name}");
                    }
                    let display = name.rsplit('/').next().unwrap_or(&name).trim_end_matches(".app");
                    cfg.apps.push(App {
                        keyword: keyword.to_lowercase(),
                        name: display.to_string(),
                        hotkey,
                        hotkey_label,
                        path,
                    });
                }
                _ => bad("unknown setting"),
            }
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

fn label(h: Hotkey, text: &str) -> String {
    let mut s = String::new();
    if h.mods & CTRL != 0 { s.push('⌃'); }
    if h.mods & OPT != 0 { s.push('⌥'); }
    if h.mods & SHIFT != 0 { s.push('⇧'); }
    if h.mods & CMD != 0 { s.push('⌘'); }
    let key = text.rsplit('+').next().unwrap_or("").trim();
    s.push_str(&match key.to_lowercase().as_str() {
        "up" => "↑".into(),
        "down" => "↓".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "space" => "Space".into(),
        k => k.to_uppercase(),
    });
    s
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
        assert_eq!(c.apps.len(), 3);
        assert_eq!(c.apps[2].keyword, "reminders");
        assert_eq!(c.apps[2].name, "Reminders");
        assert_eq!(c.apps[0].hotkey_label, "⌥J");
        assert_eq!(c.win.len(), 4);
        assert!(matches!(c.lookup(126, OPT | CMD), Some(Action::Win(Win::Next))));
        assert!(matches!(c.lookup(38, OPT), Some(Action::App(0))));
        assert!(c.lookup(38, CMD).is_none());
    }

    #[test]
    fn user_style_lines() {
        let c = Config::parse("app reminders opt+l Reminders \nfull    opt+arrow up\nleft opt+left arrow\n");
        assert_eq!(c.apps[0].name, "Reminders");
        assert_eq!(c.win, vec![
            (Hotkey { key: 126, mods: OPT }, Win::Full),
            (Hotkey { key: 123, mods: OPT }, Win::Left),
        ]);
    }
}
