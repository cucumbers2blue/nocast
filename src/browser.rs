// Websites: jump to the browser tab that already has the site open, or open it.
// Talks to Safari, or Chrome with `browser chrome`, via AppleScript (macOS asks once for permission).

use crate::config::Browser;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, AnyThread};
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::{NSAppleEventDescriptor, NSAppleScript, NSDictionary, NSNumber, NSString};

impl Browser {
    fn id(self) -> &'static str {
        match self {
            Browser::Safari => "com.apple.Safari",
            Browser::Chrome => "com.google.Chrome",
        }
    }
}

/// The AppleScript name, also used in logs.
impl std::fmt::Display for Browser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Browser::Safari => "Safari",
            Browser::Chrome => "Google Chrome",
        })
    }
}

pub fn open(url: &str, b: Browser) {
    let id = b.id();
    let running = !NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(id)).is_empty();
    debug!("{b}: {url} ({b} running: {running})");
    if !running {
        let _ = std::process::Command::new("/usr/bin/open").args(["-b", id, url]).spawn();
        return;
    }
    let front = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|a| a.bundleIdentifier())
        .is_some_and(|front| front.to_string() == id);

    if !allowed(b) {
        log!("nocast isn't allowed to control {b}. Turn it on in System Settings > Privacy & Security > Automation > nocast > {b}");
    }

    // If talking to the browser fails for any reason, still open the site.
    let fallback = || {
        let _ = std::process::Command::new("/usr/bin/open").args(["-b", id, url]).spawn();
    };
    let Some(listing) = run(b, &list_tabs(b)) else {
        debug!("{b}: couldn't list tabs, opening the URL instead");
        return fallback();
    };
    let tabs = Tabs::parse(&listing);
    let picked = tabs.pick(url, front);
    debug!(
        "{b}: {} tabs, {} match, front tab {:?}, {b} in front: {front} -> {}",
        tabs.tabs.len(),
        tabs.tabs.iter().filter(|(_, _, u)| matches(u, url)).count(),
        tabs.front,
        picked.map_or("new tab".into(), |(w, t)| format!("window {w} tab {t}"))
    );
    match picked {
        // Found: only switch tabs. Never open the URL again, which would reload it.
        Some((window, tab)) => {
            let select = match b {
                Browser::Safari => format!("set current tab of w to tab {tab} of w"),
                Browser::Chrome => format!("set active tab index of w to {tab}"),
            };
            let r = run(b, &format!(
                r#"tell application "{b}"
                    with timeout of 30 seconds
                        set w to window id {window}
                        try
                            if miniaturized of w then set miniaturized of w to false
                        end try
                        {select}
                        try
                            set index of w to 1
                        end try
                        activate
                    end timeout
                end tell
                return "ok""#
            ));
            if r.is_none() {
                debug!("{b}: couldn't switch to the tab");
            }
        }
        None => {
            let url = url.replace('\\', "\\\\").replace('"', "\\\"");
            // A new Chrome tab is selected by itself.
            let (new_tab, new_window) = match b {
                Browser::Safari => (
                    format!(r#"tell window 1 to set current tab to (make new tab at end of tabs with properties {{URL:"{url}"}})"#),
                    format!(r#"make new document with properties {{URL:"{url}"}}"#),
                ),
                Browser::Chrome => (
                    format!(r#"make new tab at end of tabs of window 1 with properties {{URL:"{url}"}}"#),
                    format!(r#"set URL of active tab of (make new window) to "{url}""#),
                ),
            };
            let r = run(b, &format!(
                r#"tell application "{b}"
                    with timeout of 30 seconds
                        try
                            {new_tab}
                        on error
                            {new_window}
                        end try
                        activate
                    end timeout
                end tell
                return "ok""#
            ));
            if r.is_none() {
                debug!("{b}: script failed, opening the URL instead");
                fallback();
            }
        }
    }
}

/// One line per tab: "window id<TAB>tab index<TAB>url". The first line is
/// "front<TAB>window id<TAB>current tab index" for the frontmost window.
fn list_tabs(b: Browser) -> String {
    let front_tab = match b {
        Browser::Safari => "index of current tab of window 1",
        Browser::Chrome => "active tab index of window 1",
    };
    format!(r#"
set tabChar to character id 9
set res to ""
tell application "{b}"
    with timeout of 30 seconds
        try
            set res to "front" & tabChar & (id of window 1) & tabChar & ({front_tab}) & linefeed
        end try
        -- Fetch the ids first: looping over "id of every window" directly
        -- makes AppleScript ask {b} for "item 1 of id of every window".
        set wids to id of every window
        repeat with n from 1 to count of wids
            set wid to item n of wids
            try
                set urls to URL of every tab of window id wid
                repeat with i from 1 to count of urls
                    set u to item i of urls
                    if u is missing value then set u to ""
                    set res to res & wid & tabChar & i & tabChar & u & linefeed
                end repeat
            end try
        end repeat
    end timeout
end tell
return res
"#)
}

fn run(b: Browser, source: &str) -> Option<String> {
    let script = NSAppleScript::initWithSource(NSAppleScript::alloc(), &NSString::from_str(source))?;
    let mut error: Option<Retained<NSDictionary<NSString, AnyObject>>> = None;
    // Returns nil on error (the generated binding assumes it never does, so call it directly).
    let result: Option<Retained<NSAppleEventDescriptor>> =
        unsafe { msg_send![&*script, executeAndReturnError: Some(&mut error)] };
    if let Some(e) = error {
        let get = |k: &str| {
            let Some(v) = e.objectForKey(&NSString::from_str(k)) else { return String::new() };
            if let Some(s) = v.downcast_ref::<NSString>() {
                s.to_string()
            } else if let Some(n) = v.downcast_ref::<NSNumber>() {
                n.integerValue().to_string()
            } else {
                String::new()
            }
        };
        let which = source.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
        log!(
            "{b} script failed: error {}: {} (script starting \"{which}\")",
            get("NSAppleScriptErrorNumber"),
            get("NSAppleScriptErrorMessage"),
        );
        return None;
    }
    Some(result.and_then(|r| r.stringValue()).map(|s| s.to_string()).unwrap_or_default())
}

/// Asks macOS whether nocast may send Apple Events to the browser, prompting if needed.
fn allowed(b: Browser) -> bool {
    use crate::ffi::{AEDesc, AECreateDesc, AEDeterminePermissionToAutomateTarget, AEDisposeDesc};
    const TYPE_BUNDLE_ID: u32 = u32::from_be_bytes(*b"bund");
    const WILDCARD: u32 = u32::from_be_bytes(*b"****");
    let id = b.id();
    let mut target = AEDesc { descriptor_type: 0, data_handle: std::ptr::null_mut() };
    let status = unsafe {
        if AECreateDesc(TYPE_BUNDLE_ID, id.as_ptr().cast(), id.len() as isize, &mut target) != 0 {
            return true; // can't tell; just try
        }
        let s = AEDeterminePermissionToAutomateTarget(&target, WILDCARD, WILDCARD, true);
        AEDisposeDesc(&mut target);
        s
    };
    debug!(
        "{b}: permission check: {status} ({})",
        match status {
            0 => "allowed".to_string(),
            -1743 => "denied".to_string(),
            -1744 => "would need to ask".to_string(),
            -600 => format!("{b} not running"),
            _ => "unknown".to_string(),
        }
    );
    status != -1743
}

#[derive(Debug, Default)]
struct Tabs {
    front: Option<(i64, i64)>,
    tabs: Vec<(i64, i64, String)>,
}

impl Tabs {
    fn parse(text: &str) -> Tabs {
        let mut t = Tabs::default();
        for line in text.lines() {
            let mut f = line.splitn(3, '\t');
            let (Some(a), Some(b), c) = (f.next(), f.next(), f.next()) else { continue };
            let Ok(b) = b.trim().parse::<i64>() else { continue };
            if a == "front" {
                if let Some(Ok(c)) = c.map(|c| c.trim().parse::<i64>()) {
                    t.front = Some((b, c));
                }
            } else if let Ok(a) = a.trim().parse::<i64>() {
                t.tabs.push((a, b, c.unwrap_or("").to_string()));
            }
        }
        t
    }

    /// The tab to show: if the browser is in front and already on a match, the next match
    /// (or the same one if it's the only one); otherwise the front window's tab if it
    /// matches, else the first match.
    fn pick(&self, url: &str, browser_front: bool) -> Option<(i64, i64)> {
        let hits: Vec<(i64, i64)> = self
            .tabs
            .iter()
            .filter(|(_, _, u)| matches(u, url))
            .map(|(w, t, _)| (*w, *t))
            .collect();
        let first = *hits.first()?;
        let Some(i) = self.front.and_then(|f| hits.iter().position(|h| *h == f)) else { return Some(first) };
        Some(if browser_front { hits[(i + 1) % hits.len()] } else { hits[i] })
    }
}

/// "https://www.github.com/foo/bar?x" matches "github.com/foo", but not "github.community".
pub fn matches(tab_url: &str, want: &str) -> bool {
    let (a, b) = (normalize(tab_url), normalize(want));
    if b.is_empty() || !a.starts_with(&b) {
        return false;
    }
    matches!(a[b.len()..].chars().next(), None | Some('/' | '?' | '#'))
}

fn normalize(url: &str) -> String {
    let u = url.trim().to_lowercase();
    let u = u.split_once("://").map_or(u.as_str(), |(_, rest)| rest);
    let u = u.strip_prefix("www.").unwrap_or(u);
    u.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_matching() {
        assert!(matches("https://mail.google.com/mail/u/0/#inbox", "https://mail.google.com"));
        assert!(matches("http://www.github.com/", "https://github.com"));
        assert!(matches("https://github.com/cucumbers2blue/nocast", "github.com/cucumbers2blue"));
        assert!(matches("https://github.com/?tab=1", "https://github.com/"));
        assert!(!matches("https://github.community", "https://github.com"));
        assert!(!matches("https://github.com/other", "https://github.com/cucumbers2blue"));
        assert!(!matches("", "https://github.com"));
        assert!(!matches("favorites://", "https://github.com"));
        // A domain alone matches every page of the site.
        assert!(matches("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s", "https://youtube.com"));
        assert!(matches("https://www.youtube.com/watch?v=dQw4w9WgXcQ", "https://www.youtube.com/"));
        assert!(matches("https://YouTube.com/@channel", "youtube.com"));
    }

    #[test]
    fn picking_tabs() {
        let t = Tabs::parse(
            "front\t7\t2\n7\t1\thttps://apple.com\n7\t2\thttps://mail.google.com/a\n7\t3\t\n9\t1\thttps://mail.google.com/b\n",
        );
        assert_eq!(t.front, Some((7, 2)));
        assert_eq!(t.tabs.len(), 4);
        let gmail = "https://mail.google.com";
        // Browser in front, on a Gmail tab: go to the next Gmail tab.
        assert_eq!(t.pick(gmail, true), Some((9, 1)));
        // Browser in the background: just bring the current Gmail tab forward.
        assert_eq!(t.pick(gmail, false), Some((7, 2)));
        // Not on a match: first match. Only one match: stay on it.
        assert_eq!(t.pick("apple.com", true), Some((7, 1)));
        assert_eq!(t.pick("apple.com", false), Some((7, 1)));
        // Not open anywhere.
        assert_eq!(t.pick("https://example.com", true), None);
    }

    #[test]
    fn scripts_name_the_browser() {
        let safari = list_tabs(Browser::Safari);
        assert!(safari.contains("tell application \"Safari\"") && safari.contains("(index of current tab of window 1)"));
        let chrome = list_tabs(Browser::Chrome);
        assert!(chrome.contains("tell application \"Google Chrome\"") && chrome.contains("(active tab index of window 1)"));
        assert_eq!(Browser::Chrome.id(), "com.google.Chrome");
    }
}
