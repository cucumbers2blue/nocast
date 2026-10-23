// The search box: a small floating panel that lists the apps from the config.
// It never activates nocast, so closing it hands focus straight back.

use crate::config::Config;
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

pub struct Launcher {
    panel: Retained<NSPanel>,
    query_label: Retained<NSTextField>,
    shown: Cell<bool>,
    state: RefCell<State>,
    _observers: RefCell<Vec<Retained<AnyObject>>>,
}

#[derive(Default)]
struct State {
    query: String,
    sel: usize,
    matches: Vec<usize>,
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

    let launcher = Launcher {
        panel,
        query_label,
        shown: Cell::new(false),
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
    pub fn toggle(&self) {
        if self.shown.get() { self.hide() } else { self.show() }
    }

    pub fn show(&self) {
        {
            let mut s = self.state.borrow_mut();
            s.query.clear();
            s.sel = 0;
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
            self.panel.setFrame_display(NSRect::new(NSPoint::new(x, top - H), NSSize::new(W, H)), false);
        }
        self.render();
        self.shown.set(true);
        self.panel.makeKeyAndOrderFront(None);
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

    fn move_sel(&self, d: isize) {
        let mut s = self.state.borrow_mut();
        let n = s.matches.len() as isize;
        if n > 0 {
            s.sel = ((s.sel as isize + d).rem_euclid(n)) as usize;
        }
        drop(s);
        self.render();
    }

    fn launch_selected(&self) {
        let app = {
            let s = self.state.borrow();
            s.matches.get(s.sel).copied()
        };
        self.hide();
        if let Some(i) = app {
            crate::open_app(i, false);
        }
    }

    fn render(&self) {
        let (query, name) = {
            let mut s = self.state.borrow_mut();
            let found = crate::with_config(|c| {
                s.matches = matches(c, &s.query);
                if s.sel >= s.matches.len() {
                    s.sel = 0;
                }
                s.matches.get(s.sel).map(|&i| c.apps[i].name.clone())
            });
            (s.query.clone(), found)
        };
        let font = NSFont::systemFontOfSize_weight(FONT, unsafe { NSFontWeightLight });
        let text = NSMutableAttributedString::new();
        if query.is_empty() {
            text.appendAttributedString(&styled("Search apps", &NSColor::tertiaryLabelColor(), &font));
        } else {
            text.appendAttributedString(&styled(&query, &NSColor::labelColor(), &font));
            if let Some(name) = name {
                text.appendAttributedString(&styled(&completion(&query, &name), &NSColor::tertiaryLabelColor(), &font));
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
}
