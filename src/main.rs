// nocast: a tiny app launcher and window manager for macOS.
//
// A keyboard event tap on its own thread catches the configured hotkeys and
// hands the matching action to the main thread, which owns all UI work.

mod config;
mod ffi;
mod launcher;
mod safari;
mod windows;

use config::{Action, Config, MODS};
use ffi::*;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSDictionary, NSError, NSNumber, NSString, NSTimer, NSURL};
use std::cell::Cell;
use std::ffi::c_void;
use std::path::Path;
use std::ptr::{null, null_mut, NonNull};
use std::sync::{Mutex, RwLock};
use std::time::SystemTime;

static CONFIG: RwLock<Config> = RwLock::new(Config { launcher: None, apps: Vec::new(), win: Vec::new() });
static MTIME: Mutex<Option<SystemTime>> = Mutex::new(None);

pub fn with_config<R>(f: impl FnOnce(&Config) -> R) -> R {
    f(&CONFIG.read().unwrap_or_else(|e| e.into_inner()))
}

/// Re-reads the config if the file changed since last time. Cheap: one stat.
fn reload_config() {
    let path = config::path();
    if !path.exists() {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(&path, config::DEFAULT);
    }
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let mut last = MTIME.lock().unwrap_or_else(|e| e.into_inner());
    if *last == mtime && mtime.is_some() {
        return;
    }
    *last = mtime;
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    *CONFIG.write().unwrap_or_else(|e| e.into_inner()) = Config::parse(&text);
}

// ---- keyboard tap (runs on its own thread) ----

thread_local! {
    static TAP: Cell<*mut c_void> = const { Cell::new(null_mut()) };
}

extern "C" fn on_event(_: *mut c_void, kind: u32, event: CGEventRef, _: *mut c_void) -> CGEventRef {
    if kind == kCGEventTapDisabledByTimeout || kind == kCGEventTapDisabledByUserInput {
        TAP.with(|t| unsafe { CGEventTapEnable(t.get(), true) });
        return event;
    }
    if kind != kCGEventKeyDown {
        return event;
    }
    let mods = unsafe { CGEventGetFlags(event) } & MODS;
    if mods == 0 {
        return event; // plain typing: let it through untouched
    }
    reload_config();
    let key = unsafe { CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode) } as u16;
    let Some(action) = with_config(|c| c.lookup(key, mods)) else { return event };
    let repeat = unsafe { CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat) } != 0;
    if !repeat {
        post_to_main(action);
    }
    null_mut() // swallow the hotkey
}

fn start_tap() {
    std::thread::spawn(|| unsafe {
        let tap = CGEventTapCreate(
            kCGSessionEventTap,
            kCGHeadInsertEventTap,
            kCGEventTapOptionDefault,
            1 << kCGEventKeyDown,
            on_event,
            null_mut(),
        );
        if tap.is_null() {
            eprintln!("nocast: could not listen for hotkeys (Accessibility permission?)");
            std::process::exit(1); // launchd restarts us
        }
        TAP.with(|t| t.set(tap));
        let source = CFMachPortCreateRunLoopSource(null(), tap, 0);
        CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
        CGEventTapEnable(tap, true);
        CFRunLoopRun();
    });
}

// ---- actions (main thread) ----

fn post_to_main(action: Action) {
    let ctx = Box::into_raw(Box::new(action)) as *mut c_void;
    unsafe { dispatch_async_f(&raw const _dispatch_main_q as *mut c_void, ctx, run_action) };
}

extern "C" fn run_action(ctx: *mut c_void) {
    let action = *unsafe { Box::from_raw(ctx as *mut Action) };
    let mtm = MainThreadMarker::new().expect("main thread");
    match action {
        Action::Launcher => {
            launcher::with(|l| l.toggle());
        }
        Action::App(i) => {
            launcher::with(|l| l.hide());
            open_app(i, true);
        }
        Action::Win(w) => windows::run(w, mtm),
    }
}

/// Opens (or focuses) an app or website. With `toggle`, hides an app that's already in front.
pub fn open_app(i: usize, toggle: bool) {
    let Some((path, name, url)) =
        with_config(|c| c.apps.get(i).map(|a| (a.path.clone(), a.name.clone(), a.url.clone())))
    else {
        return;
    };
    if let Some(url) = url {
        safari::open(&url);
        return;
    }
    let ws = NSWorkspace::sharedWorkspace();
    let Some(path) = path else {
        // Not found in the usual folders; ask macOS to find it by name.
        open_by_name(name);
        return;
    };
    if toggle
        && let Some(front) = ws.frontmostApplication()
        && let Some(front_path) = front.bundleURL().and_then(|u| u.path())
        && same_file(Path::new(&front_path.to_string()), &path)
    {
        front.hide();
        return;
    }
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let done = block2::RcBlock::new(move |_: *mut NSRunningApplication, err: *mut NSError| {
        if let Some(err) = unsafe { err.as_ref() } {
            eprintln!("nocast: could not open {name}: {}", err.localizedDescription());
            open_by_name(name.clone());
        }
    });
    ws.openApplicationAtURL_configuration_completionHandler(&url, &NSWorkspaceOpenConfiguration::configuration(), Some(&done));
}

fn open_by_name(name: String) {
    std::thread::spawn(move || {
        match std::process::Command::new("/usr/bin/open").args(["-a", &name]).output() {
            Ok(out) if !out.status.success() => {
                eprintln!("nocast: open -a {name}: {}", String::from_utf8_lossy(&out.stderr).trim())
            }
            Err(e) => eprintln!("nocast: open -a {name}: {e}"),
            _ => {}
        }
    });
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

// ---- startup ----

fn main() {
    let mtm = MainThreadMarker::new().expect("main thread");
    reload_config();

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory); // no Dock icon
    launcher::init(mtm);

    // Hotkeys and window moving need Accessibility. Ask once, then wait for it.
    if unsafe { AXIsProcessTrusted() } {
        start_tap();
    } else {
        let key = NSString::from_str("AXTrustedCheckOptionPrompt");
        let yes = NSNumber::new_bool(true);
        let opts = NSDictionary::from_slices(&[&*key], &[&*yes]);
        unsafe { AXIsProcessTrustedWithOptions(&*opts as *const _ as CFTypeRef) };
        let wait = block2::RcBlock::new(|timer: NonNull<NSTimer>| {
            if unsafe { AXIsProcessTrusted() } {
                unsafe { timer.as_ref().invalidate() };
                start_tap();
            }
        });
        unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.0, true, &wait) };
    }

    app.run();
}
