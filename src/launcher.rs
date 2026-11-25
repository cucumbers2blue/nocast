// The search box: a small floating panel that lists the apps from the config.
// It never activates nocast, so closing it hands focus straight back.
// The same panel also searches YouTube and lists browser tabs.

use crate::browser::{OpenTabs, TabAction};
use crate::config::{Browser, Config};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::*;
use objc2_foundation::*;
use std::cell::{Cell, OnceCell, RefCell};
use std::ptr::{null_mut, NonNull};

const W: f64 = 560.0;
const H: f64 = 58.0;
const FONT: f64 = 22.0;
const TAB_ROWS: usize = 9;
const ROW_HEIGHT: f64 = 30.0;
const TABS_HEADER_HEIGHT: f64 = 38.0;

/// The window's own corner radius, so the mode border follows the panel's edge.
fn corner_radius() -> f64 {
    if NSProcessInfo::processInfo().operatingSystemVersion().majorVersion >= 26 { 16.0 } else { 10.0 }
}

pub struct Launcher {
    panel: Retained<NSPanel>,
    query_label: Retained<NSTextField>,
    shortcut_label: Retained<NSTextField>,
    mode_border: Retained<NSBox>,
    tab_list: Retained<NSView>,
    tab_header: Retained<NSTextField>,
    tab_highlight: Retained<NSBox>,
    tab_rows: Vec<(Retained<NSTextField>, Retained<NSTextField>)>,
    shown: Cell<bool>,
    mode: Cell<Mode>,
    state: RefCell<State>,
    _observers: RefCell<Vec<Retained<AnyObject>>>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Apps,
    YouTube,
    BrowserTabs(Browser),
}

#[derive(Default)]
struct State {
    query: String,
    sel: usize,
    matches: Vec<usize>,
    browser_tabs: Option<OpenTabs>,
}

thread_local! {
    static LAUNCHER: OnceCell<Launcher> = const { OnceCell::new() };
}

/// Apps matching the query, best first: name prefix, word prefix, anywhere, letters in order.
pub fn matches(cfg: &Config, query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    if q.is_empty() {
        return vec![];
    }
    let mut scored: Vec<(u8, usize)> = cfg
        .apps
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            let name = a.name.to_lowercase();
            let score = if name.starts_with(&q) {
                0
            } else if name.split(' ').any(|w| w.starts_with(&q)) {
                1
            } else if name.contains(&q) {
                2
            } else if is_subsequence(&q, &name) {
                3
            } else {
                return None;
            };
            Some((score, i))
        })
        .collect();
    scored.sort();
    scored.into_iter().map(|(_, i)| i).collect()
}

/// What the field shows: the typed text, then the rest of the best match in grey.
/// "lo" + Logic Pro -> ("lo", "gic Pro"); "pro" + Logic Pro -> ("pro", "  —  Logic Pro").
pub fn completion(query: &str, name: &str) -> String {
    if name.to_lowercase().starts_with(&query.to_lowercase()) {
        name.chars().skip(query.chars().count()).collect()
    } else {
        format!("  —  {name}")
    }
}

fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

/// In the normal launcher, "?" (ignoring spaces) opens the config file.
fn is_config_query(query: &str) -> bool {
    query.trim() == "?"
}

pub fn init(mtm: MainThreadMarker) {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::FullSizeContentView
        | NSWindowStyleMask::NonactivatingPanel;
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(W, H));
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        rect,
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setLevel(101); // above menus, like Spotlight
    panel.setHidesOnDeactivate(false);
    panel.setBecomesKeyOnlyIfNeeded(false);
    panel.setTitlebarAppearsTransparent(true);
    panel.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    panel.setMovable(false);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::MoveToActiveSpace
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    for b in [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton] {
        if let Some(b) = panel.standardWindowButton(b) {
            b.setHidden(true);
        }
    }

    let content = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), rect);
    content.setMaterial(NSVisualEffectMaterial::Popover);
    content.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    content.setState(NSVisualEffectState::Active);
    panel.setContentView(Some(&content));

    let query_label = NSTextField::labelWithString(&NSString::new(), mtm);
    query_label.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    query_label.setFrame(NSRect::new(NSPoint::new(20.0, (H - 28.0) / 2.0), NSSize::new(W - 40.0, 28.0)));
    content.addSubview(&query_label);

    let shortcut_label = NSTextField::labelWithString(&NSString::new(), mtm);
    shortcut_label.setFont(Some(&NSFont::systemFontOfSize(17.0)));
    shortcut_label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    shortcut_label.setAlignment(NSTextAlignment::Right);
    content.addSubview(&shortcut_label);

    let resize = NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable;
    let tab_list = NSView::initWithFrame(NSView::alloc(mtm), rect);
    tab_list.setAutoresizingMask(resize);
    tab_list.setHidden(true);
    content.addSubview(&tab_list);
    let tab_header = NSTextField::labelWithString(&NSString::new(), mtm);
    tab_header.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    tab_header.setTextColor(Some(&NSColor::secondaryLabelColor()));
    tab_list.addSubview(&tab_header);
    let tab_highlight = NSBox::initWithFrame(NSBox::alloc(mtm), NSRect::ZERO);
    tab_highlight.setBoxType(NSBoxType::Custom);
    tab_highlight.setTitlePosition(NSTitlePosition::NoTitle);
    tab_highlight.setBorderWidth(0.0);
    tab_highlight.setCornerRadius(6.0);
    tab_highlight.setFillColor(&NSColor::systemBlueColor().colorWithAlphaComponent(0.22));
    tab_list.addSubview(&tab_highlight);
    let tab_rows = (0..TAB_ROWS).map(|_| {
        let title = NSTextField::labelWithString(&NSString::new(), mtm);
        title.setFont(Some(&NSFont::systemFontOfSize(16.0)));
        title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        tab_list.addSubview(&title);
        let number = NSTextField::labelWithString(&NSString::new(), mtm);
        number.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(14.0, unsafe { NSFontWeightRegular })));
        number.setTextColor(Some(&NSColor::secondaryLabelColor()));
        number.setAlignment(NSTextAlignment::Right);
        tab_list.addSubview(&number);
        (title, number)
    }).collect();

    let mode_border = NSBox::initWithFrame(NSBox::alloc(mtm), rect);
    mode_border.setAutoresizingMask(resize);
    mode_border.setBoxType(NSBoxType::Custom);
    mode_border.setTitlePosition(NSTitlePosition::NoTitle);
    mode_border.setFillColor(&NSColor::clearColor());
    mode_border.setBorderWidth(1.5);
    mode_border.setCornerRadius(corner_radius());
    mode_border.setWantsLayer(true);
    mode_border.setHidden(true);
    content.addSubview(&mode_border);

    let launcher = Launcher {
        panel,
        query_label,
        shortcut_label,
        mode_border,
        tab_list,
        tab_header,
        tab_highlight,
        tab_rows,
        shown: Cell::new(false),
        mode: Cell::new(Mode::Apps),
        state: RefCell::new(State::default()),
        _observers: RefCell::new(vec![]),
    };

    // Typing goes through here while the panel is open.
    let monitor = block2::RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        let ev = unsafe { event.as_ref() };
        if with(|l| l.on_key(ev)) == Some(true) { null_mut() } else { event.as_ptr() }
    });
    let m = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &monitor) };

    // Click elsewhere: close.
    let on_resign = block2::RcBlock::new(|_: NonNull<NSNotification>| {
        with(|l| l.hide());
    });
    let o = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSWindowDidResignKeyNotification),
            Some(&launcher.panel),
            None,
            &on_resign,
        )
    };
    {
        let mut obs = launcher._observers.borrow_mut();
        obs.extend(m);
        let o: Retained<AnyObject> = o.into();
        obs.push(o);
    }
    LAUNCHER.with(|l| {
        let _ = l.set(launcher);
    });
}

pub fn with<R>(f: impl FnOnce(&Launcher) -> R) -> Option<R> {
    LAUNCHER.with(|l| l.get().map(f))
}

impl Launcher {
    /// Toggle the active mode or switch to a different one.
    pub fn toggle(&self, mode: Mode) {
        if self.shown.get() && self.mode.get() == mode { self.hide() } else { self.show(mode) }
    }

    fn show(&self, mode: Mode) {
        self.mode.set(mode);
        *self.state.borrow_mut() = State::default();
        let in_tabs = matches!(mode, Mode::BrowserTabs(_));
        self.query_label.setHidden(in_tabs);
        self.tab_list.setHidden(!in_tabs);
        self.mode_border.setHidden(mode == Mode::Apps);
        if mode != Mode::Apps {
            let (r, g, b) = if in_tabs { (0.19, 0.53, 1.0) } else { (1.0, 0.0, 0.0) };
            self.mode_border.setBorderColor(&NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 0.55));
            let glow = NSShadow::new();
            glow.setShadowColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 0.5)));
            glow.setShadowBlurRadius(6.0);
            glow.setShadowOffset(NSSize::new(0.0, 0.0));
            self.mode_border.setShadow(Some(&glow));
        }
        if let Mode::BrowserTabs(browser) = mode {
            self.shortcut_label.setHidden(true);
            self.refresh_browser_tabs(browser);
        } else {
            self.set_height(H);
            self.render();
        }
        // Center horizontally on the screen with the mouse, upper third.
        let mtm = self.panel.mtm();
        let mouse = NSEvent::mouseLocation();
        let screens = NSScreen::screens(mtm);
        let screen = screens
            .iter()
            .find(|s| {
                let f = s.frame();
                mouse.x >= f.origin.x && mouse.x < f.origin.x + f.size.width
                    && mouse.y >= f.origin.y && mouse.y < f.origin.y + f.size.height
            })
            .or_else(|| screens.firstObject());
        if let Some(screen) = screen {
            let f = screen.frame();
            let top = f.origin.y + f.size.height * 0.72;
            let x = f.origin.x + ((f.size.width - W) / 2.0).round();
            let height = self.panel.frame().size.height;
            self.panel.setFrame_display(NSRect::new(NSPoint::new(x, top - height), NSSize::new(W, height)), false);
        }
        self.shown.set(true);
        self.panel.makeKeyAndOrderFront(None);
    }

    fn set_height(&self, height: f64) {
        let mut frame = self.panel.frame();
        if frame.size.height != height {
            frame.origin.y += frame.size.height - height;
            frame.size.height = height;
            self.panel.setFrame_display(frame, false);
        }
    }

    pub fn hide(&self) {
        if self.shown.replace(false) {
            self.panel.orderOut(None);
        }
    }

    /// Returns true when the key was handled (and should not beep).
    fn on_key(&self, ev: &NSEvent) -> bool {
        if !self.shown.get() {
            return false;
        }
        if let Mode::BrowserTabs(browser) = self.mode.get() {
            return self.on_tab_key(ev, browser);
        }
        let flags = ev.modifierFlags();
        match ev.keyCode() {
            53 => self.hide(),                       // escape
            36 | 76 => self.launch_selected(),       // return, enter
            126 => self.move_sel(-1),                // up: previous match
            125 | 48 => self.move_sel(1),            // down, tab: next match
            51 => {                                  // delete
                let mut s = self.state.borrow_mut();
                if flags.contains(NSEventModifierFlags::Command) { s.query.clear() } else { s.query.pop(); }
                s.sel = 0;
                drop(s);
                self.render();
            }
            _ if flags.intersects(NSEventModifierFlags::Command | NSEventModifierFlags::Control) => {
                return false;
            }
            _ => {
                let typed = ev.characters().map(|c| c.to_string()).unwrap_or_default();
                let typed: String = typed.chars().filter(|c| !c.is_control()).collect();
                if !typed.is_empty() {
                    let mut s = self.state.borrow_mut();
                    s.query.push_str(&typed);
                    s.sel = 0;
                    drop(s);
                    self.render();
                }
            }
        }
        true
    }

    fn on_tab_key(&self, ev: &NSEvent, browser: Browser) -> bool {
        let modifiers = NSEventModifierFlags::Command | NSEventModifierFlags::Control | NSEventModifierFlags::Option;
        if ev.modifierFlags().intersects(modifiers) {
            return false;
        }
        let selected = self.state.borrow().sel;
        match ev.keyCode() {
            53 => self.hide(),
            36 | 76 => self.act_on_tab(browser, selected, TabAction::Select),
            125 => self.move_sel(1),
            126 => self.move_sel(-1),
            _ => {
                let key = ev.charactersIgnoringModifiers().and_then(|c| c.to_string().chars().next());
                match key {
                    Some('j' | 'J') => self.move_sel(1),
                    Some('k' | 'K') => self.move_sel(-1),
                    Some('d' | 'D') => {
                        if !ev.isARepeat() {
                            self.act_on_tab(browser, selected, TabAction::Close);
                        }
                    }
                    Some(key @ '1'..='9') => {
                        let index = selected / TAB_ROWS * TAB_ROWS + (key as usize - '1' as usize);
                        self.act_on_tab(browser, index, TabAction::Select);
                    }
                    _ => {}
                }
            }
        }
        true
    }

    fn act_on_tab(&self, browser: Browser, index: usize, action: TabAction) {
        let success = {
            let state = self.state.borrow();
            let Some(pages) = &state.browser_tabs else { return };
            if index >= pages.tabs.len() {
                return;
            }
            pages.act(browser, index, action)
        };
        if success && action == TabAction::Select {
            self.hide();
        } else {
            self.refresh_browser_tabs(browser);
        }
    }

    fn refresh_browser_tabs(&self, browser: Browser) {
        let tabs = crate::browser::open_tabs(browser);
        let count = tabs.as_ref().map_or(0, |pages| pages.tabs.len());
        let mut state = self.state.borrow_mut();
        state.sel = state.sel.min(count.saturating_sub(1));
        state.browser_tabs = tabs;
        drop(state);
        self.render();
    }

    fn render_browser_tabs(&self, browser: Browser) {
        let state = self.state.borrow();
        let tabs = state.browser_tabs.as_ref().map_or(&[][..], |pages| pages.tabs.as_slice());
        let start = state.sel / TAB_ROWS * TAB_ROWS;
        let end = (start + TAB_ROWS).min(tabs.len());
        let visible = &tabs[start..end];
        let height = TABS_HEADER_HEIGHT + ROW_HEIGHT * visible.len().max(1) as f64 + 10.0;
        self.set_height(height);

        let heading = if tabs.len() > TAB_ROWS {
            format!("{browser}  ·  {}–{end} of {}", start + 1, tabs.len())
        } else {
            browser.to_string()
        };
        self.tab_header.setStringValue(&NSString::from_str(&heading));
        self.tab_header.setFrame(NSRect::new(NSPoint::new(20.0, height - 29.0), NSSize::new(W - 40.0, 18.0)));
        self.tab_highlight.setHidden(visible.is_empty());
        for (i, (title, number)) in self.tab_rows.iter().enumerate() {
            let tab = visible.get(i);
            let empty = i == 0 && visible.is_empty();
            title.setHidden(tab.is_none() && !empty);
            number.setHidden(tab.is_none());
            if tab.is_none() && !empty {
                continue;
            }
            let y = height - TABS_HEADER_HEIGHT - ROW_HEIGHT * (i + 1) as f64;
            let text = if let Some(tab) = tab {
                tab.label()
            } else if state.browser_tabs.is_some() {
                "No open tabs"
            } else {
                "Couldn’t read open tabs"
            };
            let text = text.replace(['\n', '\r', '\t'], " ");
            title.setStringValue(&NSString::from_str(&text));
            title.setFrame(NSRect::new(NSPoint::new(20.0, y + 3.0), NSSize::new(W - 72.0, 24.0)));
            if tab.is_some() {
                number.setStringValue(&NSString::from_str(&(i + 1).to_string()));
                number.setFrame(NSRect::new(NSPoint::new(W - 42.0, y + 4.0), NSSize::new(22.0, 22.0)));
                if start + i == state.sel {
                    self.tab_highlight.setFrame(NSRect::new(
                        NSPoint::new(12.0, y),
                        NSSize::new(W - 24.0, ROW_HEIGHT),
                    ));
                }
            }
        }
    }

    fn move_sel(&self, d: isize) {
        let mut s = self.state.borrow_mut();
        let n = match self.mode.get() {
            Mode::BrowserTabs(_) => s.browser_tabs.as_ref().map_or(0, |pages| pages.tabs.len()),
            _ => s.matches.len(),
        } as isize;
        if n > 0 {
            s.sel = ((s.sel as isize + d).rem_euclid(n)) as usize;
        }
        drop(s);
        self.render();
    }

    fn launch_selected(&self) {
        if self.mode.get() == Mode::YouTube {
            let query = self.state.borrow().query.trim().to_string();
            self.hide();
            debug!("youtube: return pressed, query {query:?}");
            if !query.is_empty() {
                crate::browser::search_youtube(&query, crate::with_config(|c| c.browser));
            }
            return;
        }
        if is_config_query(&self.state.borrow().query) {
            self.hide();
            debug!("launcher: open config");
            crate::open_config();
            return;
        }
        let app = {
            let s = self.state.borrow();
            s.matches.get(s.sel).copied()
        };
        self.hide();
        debug!("launcher: return pressed, selection {app:?}");
        if let Some(i) = app {
            crate::open_app(i, false);
        }
    }

    fn render(&self) {
        let mode = self.mode.get();
        if let Mode::BrowserTabs(browser) = mode {
            self.render_browser_tabs(browser);
            return;
        }
        let youtube = mode == Mode::YouTube;
        let (query, name) = {
            let mut s = self.state.borrow_mut();
            let open_config = !youtube && is_config_query(&s.query);
            let found = crate::with_config(|c| {
                s.matches = if youtube || open_config { vec![] } else { matches(c, &s.query) };
                if s.sel >= s.matches.len() {
                    s.sel = 0;
                }
                s.matches.get(s.sel).map(|&i| {
                    let a = &c.apps[i];
                    (a.name.clone(), a.path.is_some() || a.url.is_some(), a.hotkey)
                })
            });
            (s.query.clone(), found)
        };
        let shortcut = name.as_ref().and_then(|(_, _, hotkey)| *hotkey)
            .map(|hotkey| hotkey.to_string()).unwrap_or_default();
        self.shortcut_label.setStringValue(&NSString::from_str(&shortcut));
        self.shortcut_label.setHidden(shortcut.is_empty());
        self.shortcut_label.sizeToFit();
        let shortcut_size = self.shortcut_label.frame().size;
        let shortcut_width = shortcut_size.width.ceil();
        let query_width = if shortcut.is_empty() { W - 40.0 } else { W - 56.0 - shortcut_width };
        self.shortcut_label.setFrame(NSRect::new(
            NSPoint::new(W - 20.0 - shortcut_width, (H - shortcut_size.height) / 2.0),
            NSSize::new(shortcut_width, shortcut_size.height),
        ));
        self.query_label.setFrame(NSRect::new(
            NSPoint::new(20.0, (H - 28.0) / 2.0), NSSize::new(query_width, 28.0),
        ));
        let font = NSFont::systemFontOfSize_weight(FONT, unsafe { NSFontWeightLight });
        let text = NSMutableAttributedString::new();
        if query.is_empty() {
            let placeholder = if youtube { "Search YouTube" } else { "Search apps" };
            text.appendAttributedString(&styled(placeholder, &NSColor::tertiaryLabelColor(), &font));
        } else {
            text.appendAttributedString(&styled(&query, &NSColor::labelColor(), &font));
            if !youtube && is_config_query(&query) {
                text.appendAttributedString(&styled("  —  Open config", &NSColor::tertiaryLabelColor(), &font));
            } else if let Some((name, found, _)) = name {
                let mut rest = completion(&query, &name);
                if !found {
                    rest.push_str("  (not found)");
                }
                text.appendAttributedString(&styled(&rest, &NSColor::tertiaryLabelColor(), &font));
            }
        }
        self.query_label.setAttributedStringValue(&text);
    }
}

fn styled(s: &str, color: &NSColor, font: &NSFont) -> Retained<NSAttributedString> {
    let keys: [&NSAttributedStringKey; 2] = unsafe { [NSForegroundColorAttributeName, NSFontAttributeName] };
    let values: [&AnyObject; 2] = [color.as_ref(), font.as_ref()];
    let attrs = NSDictionary::from_slices(&keys, &values);
    unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &NSString::from_str(s), Some(&attrs)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking() {
        let c = Config::parse("Safari\nNotes\nReminders\nLogic Pro\nTerminal\n");
        let names = |q: &str| matches(&c, q).iter().map(|&i| c.apps[i].name.clone()).collect::<Vec<_>>();
        assert!(names("").is_empty());
        assert_eq!(names("saf"), ["Safari"]);
        assert_eq!(names("re"), ["Reminders"]);
        assert_eq!(names("pro"), ["Logic Pro"]);
        assert_eq!(names("t"), ["Terminal", "Notes"]);
        assert_eq!(names("rmd"), ["Reminders"]);
        assert!(names("xyz").is_empty());
    }

    #[test]
    fn inline_completion() {
        assert_eq!(completion("lo", "Logic Pro"), "gic Pro");
        assert_eq!(completion("LO", "Logic Pro"), "gic Pro");
        assert_eq!(completion("pro", "Logic Pro"), "  —  Logic Pro");
    }

    #[test]
    fn question_opens_config() {
        assert!(is_config_query("?"));
        assert!(is_config_query("  ? "));
        assert!(!is_config_query(""));
        assert!(!is_config_query("??"));
        assert!(!is_config_query("?foo"));
        assert!(!is_config_query("config"));
    }
}
