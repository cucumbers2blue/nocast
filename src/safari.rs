// Websites: jump to the Safari tab that already has the site open, or open it.
// Talks to Safari with AppleScript (macOS asks once for permission).

use objc2::AnyThread;
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::{NSAppleScript, NSString};

const SAFARI: &str = "com.apple.Safari";

pub fn open(url: &str) {
    let running = !NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(SAFARI)).is_empty();
    if !running {
        let _ = std::process::Command::new("/usr/bin/open").args(["-b", SAFARI, url]).spawn();
        return;
    }
    let front = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|a| a.bundleIdentifier())
        .is_some_and(|id| id.to_string() == SAFARI);

    // If talking to Safari fails for any reason, still open the site.
    let fallback = || {
        let _ = std::process::Command::new("/usr/bin/open").args(["-b", SAFARI, url]).spawn();
    };
    let Some(listing) = run(LIST_TABS) else { return fallback() };
    let tabs = Tabs::parse(&listing);
    let done = match tabs.pick(url, front) {
        Some((window, tab)) => run(&format!(
                r#"tell application "Safari"
                    with timeout of 30 seconds
                        set w to window id {window}
                        if miniaturized of w then set miniaturized of w to false
                        set current tab of w to tab {tab} of w
                        set index of w to 1
                        activate
                    end timeout
                end tell"#
        )),
        None => {
            let url = url.replace('\\', "\\\\").replace('"', "\\\"");
            run(&format!(
                r#"tell application "Safari"
                    with timeout of 30 seconds
                        try
                            tell window 1 to set current tab to (make new tab at end of tabs with properties {{URL:"{url}"}})
                        on error
                            make new document with properties {{URL:"{url}"}}
                        end try
                        activate
                    end timeout
                end tell"#
            ))
        }
    };
    if done.is_none() {
        fallback();
    }
}

/// One line per tab: "window id<TAB>tab index<TAB>url". The first line is
/// "front<TAB>window id<TAB>current tab index" for the frontmost window.
const LIST_TABS: &str = r#"
set sep to character id 9
set out to ""
tell application "Safari"
    with timeout of 30 seconds
        try
            set out to "front" & sep & (id of window 1) & sep & (index of current tab of window 1) & linefeed
        end try
        repeat with widRef in (id of every window)
            set wid to contents of widRef
            try
                set urls to URL of every tab of window id wid
                repeat with i from 1 to count of urls
                    set u to item i of urls
                    if u is missing value then set u to ""
                    set out to out & wid & sep & i & sep & u & linefeed
                end repeat
            end try
        end repeat
    end timeout
end tell
return out
"#;

fn run(source: &str) -> Option<String> {
    let script = NSAppleScript::initWithSource(NSAppleScript::alloc(), &NSString::from_str(source))?;
    let mut error = None;
    let result = unsafe { script.executeAndReturnError(Some(&mut error)) };
    if let Some(e) = error {
        let msg = e.objectForKey(&NSString::from_str("NSAppleScriptErrorMessage")).map(|m| format!("{m:?}"));
        eprintln!("nocast: Safari script failed: {}", msg.unwrap_or_else(|| format!("{e:?}")));
        return None;
    }
    Some(result.stringValue().map(|s| s.to_string()).unwrap_or_default())
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

    /// The tab to show: if Safari is in front and already on a match, the next match
    /// (or the same one if it's the only one); otherwise the front window's tab if it
    /// matches, else the first match.
    fn pick(&self, url: &str, safari_front: bool) -> Option<(i64, i64)> {
        let hits: Vec<(i64, i64)> = self
            .tabs
            .iter()
            .filter(|(_, _, u)| matches(u, url))
            .map(|(w, t, _)| (*w, *t))
            .collect();
        let first = *hits.first()?;
        let Some(i) = self.front.and_then(|f| hits.iter().position(|h| *h == f)) else { return Some(first) };
        Some(if safari_front { hits[(i + 1) % hits.len()] } else { hits[i] })
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
    }

    #[test]
    fn picking_tabs() {
        let t = Tabs::parse(
            "front\t7\t2\n7\t1\thttps://apple.com\n7\t2\thttps://mail.google.com/a\n7\t3\t\n9\t1\thttps://mail.google.com/b\n",
        );
        assert_eq!(t.front, Some((7, 2)));
        assert_eq!(t.tabs.len(), 4);
        let gmail = "https://mail.google.com";
        // Safari in front, on a Gmail tab: go to the next Gmail tab.
        assert_eq!(t.pick(gmail, true), Some((9, 1)));
        // Safari in the background: just bring the current Gmail tab forward.
        assert_eq!(t.pick(gmail, false), Some((7, 2)));
        // Not on a match: first match. Only one match: stay on it.
        assert_eq!(t.pick("apple.com", true), Some((7, 1)));
        assert_eq!(t.pick("apple.com", false), Some((7, 1)));
        // Not open anywhere.
        assert_eq!(t.pick("https://example.com", true), None);
    }
}
