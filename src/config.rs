// Config file: ~/.config/nocast.conf. One setting per line, `#` starts a comment.
//
//   launcher  cmd+space
//   Safari    opt+j          (an app, with an optional hotkey)
//   Logic Pro                (an app you only search for)
//   full      opt+up         (also: left, right, next)
//   browser   chrome         (websites in Chrome instead of Safari)

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

# websites: a name, the address, an optional hotkey.
# If the site is already open in Safari, nocast switches to that tab.
# For Chrome instead, add a line: browser chrome
# Gmail   https://mail.google.com   opt+g

full    opt+up
left    opt+left
right   opt+right
next    opt+cmd+up

# Log every step. Watch it live with: open -a Console ~/Library/Logs/nocast.log
# debug
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

impl std::fmt::Display for Hotkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (flag, symbol) in [(CTRL, "⌃"), (OPT, "⌥"), (SHIFT, "⇧"), (CMD, "⌘")] {
            if self.mods & flag != 0 {
                f.write_str(symbol)?;
            }
        }
        let key = match self.key {
            36 => "↩", 48 => "⇥", 49 => "Space", 51 => "⌫", 53 => "⎋",
            123 => "←", 124 => "→", 125 => "↓", 126 => "↑",
            122 => "F1", 120 => "F2", 99 => "F3", 118 => "F4", 96 => "F5", 97 => "F6",
            98 => "F7", 100 => "F8", 101 => "F9", 109 => "F10", 103 => "F11", 111 => "F12",
            _ => {
                // Use the same US key positions as the config parser.
                if let Some(c) = "abcdefghijklmnopqrstuvwxyz0123456789-=[];',./\\`"
                    .chars()
                    .find(|c| keycode(&c.to_string()) == Some(self.key))
                {
                    return write!(f, "{}", c.to_ascii_uppercase());
                }
                "?"
            }
        };
        f.write_str(key)
    }
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

/// Where websites open: `browser safari` (the default) or `browser chrome`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Browser {
    #[default]
    Safari,
    Chrome,
}

/// An app, or a website when `url` is set.
pub struct App {
    pub name: String,
    pub hotkey: Option<Hotkey>,
    pub path: Option<PathBuf>,
    pub url: Option<String>,
}

#[derive(Default)]
pub struct Config {
    pub launcher: Option<Hotkey>,
    pub apps: Vec<App>,
    pub win: Vec<(Hotkey, Win)>,
    pub debug: bool,
    pub browser: Browser,
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
            if line.eq_ignore_ascii_case("debug") {
                cfg.debug = true;
                continue;
            }
            if first.eq_ignore_ascii_case("browser") && !rest.trim().is_empty() {
                match rest.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase().as_str() {
                    "safari" => cfg.browser = Browser::Safari,
                    "chrome" | "google chrome" => cfg.browser = Browser::Chrome,
                    _ => log!("config line {}: unknown browser (use safari or chrome): {raw}", n + 1),
                }
                continue;
            }
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

            // Anything else is an app or website: a name, an optional URL, an optional hotkey.
            let mut words: Vec<&str> = line.split_whitespace().collect();
            if words.len() > 2 && words[0] == "app" {
                words.drain(..2); // old format: "app keyword [hotkey] Name"
            }
            let url = words.iter().position(|w| w.contains("://") || looks_like_domain(w)).map(|i| {
                let w = words.remove(i);
                if w.contains("://") { w.to_string() } else { format!("https://{w}") }
            });
            let mut hotkey = None;
            if let Some(i) = words.iter().position(|w| w.contains('+')) {
                hotkey = parse_hotkey(words[i]);
                if hotkey.is_none() {
                    log!("config line {}: bad hotkey: {raw}", n + 1);
                }
                words.remove(i);
            }
            let name = words.join(" ");
            if let Some(url) = url {
                // No name given: use the site's address, e.g. "mail.google.com".
                let name = if name.is_empty() {
                    url.split_once("://").map_or(url.as_str(), |(_, r)| r).trim_end_matches('/').to_string()
                } else {
                    name
                };
                cfg.apps.push(App { name, hotkey, path: None, url: Some(url) });
                continue;
            }
            if name.is_empty() {
                continue;
            }
            let path = find_app(&name);
            if path.is_none() {
                log!("app not found: {name}");
            }
            let display = name.rsplit('/').next().unwrap_or(&name).trim_end_matches(".app");
            cfg.apps.push(App { name: display.to_string(), hotkey, path, url: None });
        }
        cfg
    }
}

/// "youtube.com" or "github.com/me": lowercase, dotted, ending in a word like "com".
/// (App names like "Logic Pro" or "Pixelmator Pro 3.5" don't qualify.)
fn looks_like_domain(w: &str) -> bool {
    let host = w.split('/').next().unwrap_or("");
    let labels: Vec<&str> = host.split('.').collect();
    let tld = labels.last().copied().unwrap_or("");
    labels.len() >= 2
        && labels.iter().all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
        && tld.len() >= 2
        && tld.chars().all(|c| c.is_ascii_lowercase())
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
/// Falls back to the first app whose name starts with it ("Logic Pro" -> "Logic Pro X.app"),
/// also looking one folder deep in /Applications.
fn find_app(name: &str) -> Option<PathBuf> {
    if name.starts_with('/') || name.starts_with('~') {
        let p = PathBuf::from(name.replacen('~', &home(), 1));
        return p.exists().then_some(p);
    }
    let name = name.trim_end_matches(".app");
    let mut dirs: Vec<PathBuf> = [
        "/Applications",
        "/System/Applications",
        "/System/Applications/Utilities",
        "/Applications/Utilities",
        "/System/Library/CoreServices",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    dirs.push(PathBuf::from(home()).join("Applications"));
    if let Some(p) = dirs.iter().map(|d| d.join(format!("{name}.app"))).find(|p| p.exists()) {
        return Some(p);
    }
    for d in ["/Applications".into(), PathBuf::from(home()).join("Applications")] {
        if let Ok(entries) = std::fs::read_dir(&d) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && p.extension().is_none() {
                    dirs.push(p); // e.g. /Applications/Adobe Photoshop 2025/
                }
            }
        }
    }
    let want = name.to_lowercase();
    for d in &dirs {
        let Ok(entries) = std::fs::read_dir(d) else { continue };
        let mut hits: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let f = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                f.ends_with(".app") && f.starts_with(&want)
            })
            .collect();
        hits.sort(); // shortest / alphabetical first
        if let Some(p) = hits.into_iter().next() {
            return Some(p);
        }
    }
    None
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
    fn shortcut_symbols() {
        for (config, expected) in [
            ("opt+j", "⌥J"), ("cmd+opt+g", "⌥⌘G"),
            ("cmd+shift+ctrl+opt+7", "⌃⌥⇧⌘7"),
            ("command+space", "⌘Space"), ("opt+up", "⌥↑"),
            ("opt+down", "⌥↓"), ("opt+left", "⌥←"), ("opt+right", "⌥→"),
            ("ctrl+return", "⌃↩"), ("ctrl+tab", "⌃⇥"),
            ("cmd+delete", "⌘⌫"), ("cmd+escape", "⌘⎋"),
        ] {
            assert_eq!(parse_hotkey(config).unwrap().to_string(), expected);
        }
        for key in "abcdefghijklmnopqrstuvwxyz0123456789-=[];',./\\`".chars() {
            let hotkey = parse_hotkey(&format!("cmd+{key}")).unwrap();
            assert_eq!(hotkey.to_string(), format!("⌘{}", key.to_ascii_uppercase()));
        }
        for n in 1..=12 {
            let hotkey = parse_hotkey(&format!("shift+f{n}")).unwrap();
            assert_eq!(hotkey.to_string(), format!("⇧F{n}"));
        }
    }

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
        assert!(!c.debug);
        assert!(Config::parse("debug\n").debug);
        assert!(matches!(c.lookup(126, OPT | CMD), Some(Action::Win(Win::Next))));
        assert!(matches!(c.lookup(38, OPT), Some(Action::App(0))));
        assert!(c.lookup(38, CMD).is_none());
    }

    #[test]
    fn browser() {
        assert_eq!(Config::parse(DEFAULT).browser, Browser::Safari);
        assert_eq!(Config::parse("browser chrome\n").browser, Browser::Chrome);
        assert_eq!(Config::parse("Browser  Google Chrome  # websites\n").browser, Browser::Chrome);
        assert_eq!(Config::parse("browser chrome\nbrowser safari\n").browser, Browser::Safari);
        let c = Config::parse("browser firefox\n");
        assert_eq!(c.browser, Browser::Safari);
        assert!(c.apps.is_empty());
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

    #[test]
    fn finds_apps_by_prefix_and_in_subfolders() {
        let home = std::env::temp_dir().join(format!("nocast-test-{}", std::process::id()));
        let apps = home.join("Applications");
        std::fs::create_dir_all(apps.join("Logic Pro X.app")).unwrap();
        std::fs::create_dir_all(apps.join("Adobe Photoshop 2025/Adobe Photoshop 2025.app")).unwrap();
        unsafe { std::env::set_var("HOME", &home) };
        assert_eq!(find_app("Logic Pro"), Some(apps.join("Logic Pro X.app")));
        // Lowercase also works (macOS may keep the typed casing in the path).
        let lower = find_app("logic pro x").unwrap();
        assert_eq!(lower.to_string_lossy().to_lowercase(), apps.join("logic pro x.app").to_string_lossy().to_lowercase());
        assert_eq!(
            find_app("Adobe Photoshop"),
            Some(apps.join("Adobe Photoshop 2025/Adobe Photoshop 2025.app"))
        );
        assert_eq!(find_app("Nope"), None);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn bare_domains() {
        let c = Config::parse("youtube youtube.com\nGitHub github.com/me opt+g\nPixelmator Pro 3.5\nNode.js\n");
        let v: Vec<(&str, Option<&str>)> = c.apps.iter().map(|a| (a.name.as_str(), a.url.as_deref())).collect();
        assert_eq!(v, [
            ("youtube", Some("https://youtube.com")),
            ("GitHub", Some("https://github.com/me")),
            ("Pixelmator Pro 3.5", None),
            ("Node.js", None),
        ]);
    }

    #[test]
    fn websites() {
        let c = Config::parse("Gmail https://mail.google.com opt+g\nhttps://github.com/a+b/\nMy Site  ctrl+opt+m  https://x.com/?q=1+2\n");
        let sites: Vec<(&str, Option<&str>, Option<Hotkey>)> =
            c.apps.iter().map(|a| (a.name.as_str(), a.url.as_deref(), a.hotkey)).collect();
        assert_eq!(sites, [
            ("Gmail", Some("https://mail.google.com"), Some(Hotkey { key: 5, mods: OPT })),
            ("github.com/a+b", Some("https://github.com/a+b/"), None),
            ("My Site", Some("https://x.com/?q=1+2"), Some(Hotkey { key: 46, mods: CTRL | OPT })),
        ]);
    }
}
