// The search box: a small floating panel that lists the apps from the config.
// It never activates nocast, so closing it hands focus straight back.

use crate::config::Config;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::*;
use objc2_foundation::*;
use std::cell::{Cell, OnceCell, RefCell};
use std::ptr::{null_mut, NonNull};

const W: f64 = 560.0;
const PAD: f64 = 10.0;
const QUERY_H: f64 = 44.0;
const ROW_H: f64 = 34.0;

pub struct Launcher {
    panel: Retained<NSPanel>,
    content: Retained<NSVisualEffectView>,
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
    rows: Vec<Retained<NSView>>,
}

thread_local! {
    static LAUNCHER: OnceCell<Launcher> = const { OnceCell::new() };
}

/// Apps matching the query, best first: keyword, then name prefix, then anywhere.
pub fn matches(cfg: &Config, query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    let mut scored: Vec<(u8, usize)> = cfg
        .apps
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            let name = a.name.to_lowercase();
            let score = if q.is_empty() || a.keyword == q {
                0
            } else if a.keyword.starts_with(&q) {
                1
            } else if name.starts_with(&q) {
                2
            } else if name.contains(&q) || a.keyword.contains(&q) {
                3
            } else if is_subsequence(&q, &name) {
                4
            } else {
                return None;
            };
            Some((score, i))
        })
        .collect();
    scored.sort();
    scored.into_iter().map(|(_, i)| i).collect()
}

fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

pub fn init(mtm: MainThreadMarker) {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::FullSizeContentView
        | NSWindowStyleMask::NonactivatingPanel;
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(W, 100.0));
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
    query_label.setFont(Some(&NSFont::systemFontOfSize_weight(22.0, unsafe { NSFontWeightLight })));
    query_label.setLineBreakMode(NSLineBreakMode::ByTruncatingHead);
    content.addSubview(&query_label);

    let launcher = Launcher {
        panel,
        content,
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
            self.panel.setFrame_display(NSRect::new(NSPoint::new(x, top - 100.0), NSSize::new(W, 100.0)), false);
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
            126 => self.move_sel(-1),                // up
            125 | 48 => self.move_sel(1),            // down, tab
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
        let mtm = self.panel.mtm();
        let (query, sel, rows) = {
            let mut s = self.state.borrow_mut();
            let rows: Vec<(String, String)> = crate::with_config(|c| {
                s.matches = matches(c, &s.query);
                s.matches
                    .iter()
                    .map(|&i| {
                        let a = &c.apps[i];
                        (a.name.clone(), format!("{}   {}", a.keyword, a.hotkey_label).trim().to_string())
                    })
                    .collect()
            });
            if s.sel >= rows.len() {
                s.sel = 0;
            }
            for v in s.rows.drain(..) {
                v.removeFromSuperview();
            }
            (s.query.clone(), s.sel, rows)
        };

        // Resize, keeping the top edge in place.
        let h = PAD + QUERY_H + if rows.is_empty() { PAD } else { PAD + rows.len() as f64 * ROW_H + PAD };
        let old = self.panel.frame();
        let top = old.origin.y + old.size.height;
        self.panel.setFrame_display(NSRect::new(NSPoint::new(old.origin.x, top - h), NSSize::new(W, h)), true);

        let q = &self.query_label;
        if query.is_empty() {
            q.setStringValue(&NSString::from_str("Search apps"));
            q.setTextColor(Some(&NSColor::tertiaryLabelColor()));
        } else {
            q.setStringValue(&NSString::from_str(&query));
            q.setTextColor(Some(&NSColor::labelColor()));
        }
        q.setFrame(NSRect::new(NSPoint::new(18.0, h - PAD - (QUERY_H + 28.0) / 2.0), NSSize::new(W - 36.0, 28.0)));

        let mut views: Vec<Retained<NSView>> = vec![];
        for (i, (name, hint)) in rows.iter().enumerate() {
            let y = h - (PAD + QUERY_H + PAD) - (i as f64 + 1.0) * ROW_H;
            let selected = i == sel;
            if selected {
                let bg = NSBox::initWithFrame(NSBox::alloc(mtm), NSRect::new(NSPoint::new(PAD, y), NSSize::new(W - 2.0 * PAD, ROW_H)));
                bg.setBoxType(NSBoxType::Custom);
                bg.setTitlePosition(NSTitlePosition::NoTitle);
                bg.setBorderWidth(0.0);
                bg.setCornerRadius(7.0);
                bg.setFillColor(&NSColor::selectedContentBackgroundColor());
                self.content.addSubview(&bg);
                views.push(Retained::into_super(bg));
            }
            let color = if selected { NSColor::alternateSelectedControlTextColor() } else { NSColor::labelColor() };
            let label = NSTextField::labelWithString(&NSString::from_str(name), mtm);
            label.setFont(Some(&NSFont::systemFontOfSize(15.0)));
            label.setTextColor(Some(&color));
            label.setFrame(NSRect::new(NSPoint::new(20.0, y + (ROW_H - 19.0) / 2.0), NSSize::new(W - 180.0, 19.0)));
            self.content.addSubview(&label);
            views.push(Retained::into_super(Retained::into_super(label)));

            let hint_color = if selected { NSColor::alternateSelectedControlTextColor() } else { NSColor::secondaryLabelColor() };
            let hint = NSTextField::labelWithString(&NSString::from_str(hint), mtm);
            hint.setFont(Some(&NSFont::systemFontOfSize(12.0)));
            hint.setTextColor(Some(&hint_color));
            hint.setAlignment(NSTextAlignment::Right);
            hint.setFrame(NSRect::new(NSPoint::new(W - 180.0, y + (ROW_H - 16.0) / 2.0), NSSize::new(160.0, 16.0)));
            self.content.addSubview(&hint);
            views.push(Retained::into_super(Retained::into_super(hint)));
        }
        self.state.borrow_mut().rows = views;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking() {
        let c = Config::parse("app saf Safari\napp notes Notes\napp reminders Reminders\napp term Terminal\n");
        let names = |q: &str| matches(&c, q).iter().map(|&i| c.apps[i].name.clone()).collect::<Vec<_>>();
        assert_eq!(names(""), ["Safari", "Notes", "Reminders", "Terminal"]);
        assert_eq!(names("saf"), ["Safari"]);
        assert_eq!(names("re"), ["Reminders"]);
        assert_eq!(names("t"), ["Terminal", "Notes"]);
        assert_eq!(names("rmd"), ["Reminders"]);
        assert!(names("xyz").is_empty());
    }
}
