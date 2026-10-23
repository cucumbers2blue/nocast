// Window actions on the focused window of the frontmost app, via Accessibility.
// Accessibility uses top-left origin coordinates; AppKit screens use bottom-left.

use crate::config::Win;
use crate::ffi::*;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSScreen, NSWorkspace};
use objc2_foundation::NSString;
use std::ffi::c_void;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn overlap(&self, o: &Rect) -> f64 {
        let w = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let h = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
    }
}

/// A Core Foundation object we own and release on drop.
struct Cf(CFTypeRef);

impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}

fn copy_attr(el: AXUIElementRef, name: &str) -> Option<Cf> {
    let name = NSString::from_str(name);
    let mut value: CFTypeRef = std::ptr::null();
    let err = unsafe { AXUIElementCopyAttributeValue(el, cf(&name), &mut value) };
    (err == 0 && !value.is_null()).then_some(Cf(value))
}

fn set_attr(el: AXUIElementRef, name: &str, value: CFTypeRef) {
    let name = NSString::from_str(name);
    unsafe { AXUIElementSetAttributeValue(el, cf(&name), value) };
}

fn cf(s: &NSString) -> CFTypeRef {
    s as *const NSString as CFTypeRef
}

fn get_frame(win: AXUIElementRef) -> Option<Rect> {
    let mut p = Point::default();
    let mut s = Size::default();
    let pos = copy_attr(win, "AXPosition")?;
    let size = copy_attr(win, "AXSize")?;
    unsafe {
        if !AXValueGetValue(pos.0, kAXValueCGPointType, &mut p as *mut _ as *mut c_void)
            || !AXValueGetValue(size.0, kAXValueCGSizeType, &mut s as *mut _ as *mut c_void)
        {
            return None;
        }
    }
    Some(Rect { x: p.x, y: p.y, w: s.w, h: s.h })
}

fn set_frame(win: AXUIElementRef, r: Rect) {
    let p = Point { x: r.x, y: r.y };
    let s = Size { w: r.w, h: r.h };
    unsafe {
        let pos = Cf(AXValueCreate(kAXValueCGPointType, &p as *const _ as *const c_void));
        let size = Cf(AXValueCreate(kAXValueCGSizeType, &s as *const _ as *const c_void));
        // Size, move, size again: the first resize can be clamped by the old screen.
        set_attr(win, "AXSize", size.0);
        set_attr(win, "AXPosition", pos.0);
        set_attr(win, "AXSize", size.0);
    }
}

/// Usable area (minus menu bar and Dock) of each screen, left to right.
fn screens(mtm: MainThreadMarker) -> Vec<Rect> {
    let list = NSScreen::screens(mtm);
    let Some(primary) = list.firstObject() else { return vec![] };
    let top = primary.frame().size.height;
    let mut out: Vec<Rect> = list
        .iter()
        .map(|s| {
            let v = s.visibleFrame();
            Rect { x: v.origin.x, y: top - (v.origin.y + v.size.height), w: v.size.width, h: v.size.height }
        })
        .collect();
    out.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    out
}

fn target(action: Win, win: Rect, screens: &[Rect]) -> Option<Rect> {
    let (cur, v) = screens
        .iter()
        .enumerate()
        .max_by(|a, b| win.overlap(a.1).total_cmp(&win.overlap(b.1)))?;
    let half = (v.w / 2.0).floor();
    Some(match action {
        Win::Full => *v,
        Win::Left => Rect { w: half, ..*v },
        Win::Right => Rect { x: v.x + half, w: v.w - half, ..*v },
        Win::Next => {
            if screens.len() < 2 {
                return None;
            }
            let n = &screens[(cur + 1) % screens.len()];
            // Keep the same relative position and size on the new screen.
            let w = (win.w / v.w * n.w).min(n.w);
            let h = (win.h / v.h * n.h).min(n.h);
            let x = (n.x + (win.x - v.x) / v.w * n.w).clamp(n.x, n.x + n.w - w);
            let y = (n.y + (win.y - v.y) / v.h * n.h).clamp(n.y, n.y + n.h - h);
            Rect { x: x.round(), y: y.round(), w: w.round(), h: h.round() }
        }
    })
}

pub fn run(action: Win, mtm: MainThreadMarker) {
    let Some(front) = NSWorkspace::sharedWorkspace().frontmostApplication() else { return };
    let app = Cf(unsafe { AXUIElementCreateApplication(front.processIdentifier()) });
    unsafe { AXUIElementSetMessagingTimeout(app.0, 1.0) };
    let Some(win) = copy_attr(app.0, "AXFocusedWindow") else { return };
    let Some(frame) = get_frame(win.0) else { return };
    let Some(to) = target(action, frame, &screens(mtm)) else { return };

    // Some apps animate Accessibility-driven resizes when this is on; turn it off meanwhile.
    let enhanced = copy_attr(app.0, "AXEnhancedUserInterface").is_some_and(|v| v.0 == unsafe { kCFBooleanTrue });
    if enhanced {
        set_attr(app.0, "AXEnhancedUserInterface", unsafe { kCFBooleanFalse });
    }
    set_frame(win.0, to);
    if enhanced {
        set_attr(app.0, "AXEnhancedUserInterface", unsafe { kCFBooleanTrue });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Rect = Rect { x: 0.0, y: 25.0, w: 1512.0, h: 920.0 };
    const B: Rect = Rect { x: 1512.0, y: -200.0, w: 2560.0, h: 1415.0 };

    #[test]
    fn halves_and_full() {
        let w = Rect { x: 100.0, y: 100.0, w: 500.0, h: 400.0 };
        assert_eq!(target(Win::Full, w, &[A, B]), Some(A));
        assert_eq!(target(Win::Left, w, &[A, B]), Some(Rect { w: 756.0, ..A }));
        assert_eq!(target(Win::Right, w, &[A, B]), Some(Rect { x: 756.0, w: 756.0, ..A }));
    }

    #[test]
    fn next_screen() {
        assert_eq!(target(Win::Next, A, &[A, B]), Some(B));
        assert_eq!(target(Win::Next, B, &[A, B]), Some(A));
        assert_eq!(target(Win::Next, A, &[A]), None);
    }
}
