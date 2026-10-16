//! Platform window chrome.
//!
//! On macOS the window uses a transparent, full-size-content title bar; an empty unified
//! toolbar makes the title bar taller so the traffic lights sit vertically centered in
//! Dr. Rocket's own title bar (the same trick native "unified toolbar" apps use).

/// Height of the native title bar region the traffic lights are centered in.
#[cfg(target_os = "macos")]
pub const NATIVE_TITLEBAR_H: f32 = 52.0;
#[cfg(not(target_os = "macos"))]
pub const NATIVE_TITLEBAR_H: f32 = 0.0;

/// Horizontal space reserved for the traffic lights.
#[cfg(target_os = "macos")]
pub const TRAFFIC_LIGHTS_W: f32 = 84.0;
#[cfg(not(target_os = "macos"))]
pub const TRAFFIC_LIGHTS_W: f32 = 0.0;

#[cfg(target_os = "macos")]
pub fn setup_window(handle: &impl raw_window_handle::HasWindowHandle) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSToolbar, NSView, NSWindowTitleVisibility, NSWindowToolbarStyle};
    use raw_window_handle::RawWindowHandle;

    let Ok(h) = handle.window_handle() else { return };
    let RawWindowHandle::AppKit(appkit) = h.as_raw() else { return };
    let Some(mtm) = MainThreadMarker::new() else { return };
    // SAFETY: the handle comes from a live winit window on the main thread.
    let view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let Some(window) = view.window() else { return };
    let toolbar = NSToolbar::new(mtm);
    window.setToolbar(Some(&toolbar));
    window.setToolbarStyle(NSWindowToolbarStyle::Unified);
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    window.setTitlebarAppearsTransparent(true);
}

#[cfg(not(target_os = "macos"))]
pub fn setup_window(_handle: &impl raw_window_handle::HasWindowHandle) {}

// ---------------------------------------------------------------------------------------
// Files opened from Finder ("Open With", double-click, `open -a`).
//
// macOS delivers these as `application:openURLs:` to the app delegate. winit's delegate does
// not implement it, so we add the method to its class at startup and queue the paths.

static OPENED: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());
static REPAINT: std::sync::OnceLock<egui::Context> = std::sync::OnceLock::new();

/// Paths macOS asked us to open since the last call.
pub fn take_opened_files() -> Vec<std::path::PathBuf> {
    OPENED.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

/// Remember the egui context so a file opened from Finder wakes the UI.
pub fn set_repaint_context(ctx: &egui::Context) {
    let _ = REPAINT.set(ctx.clone());
}

#[cfg(target_os = "macos")]
mod open_files {
    use super::{OPENED, REPAINT};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Imp, NSObject, Sel};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::{NSApplication, NSApplicationWillFinishLaunchingNotification};
    use objc2_foundation::{NSArray, NSNotification, NSNotificationCenter, NSURL};

    unsafe extern "C-unwind" fn open_urls(_this: &AnyObject, _cmd: Sel, _app: &AnyObject, urls: &NSArray<NSURL>) {
        let paths: Vec<std::path::PathBuf> = urls.iter().filter_map(|u| u.path()).map(|p| std::path::PathBuf::from(p.to_string())).collect();
        if let Ok(mut q) = OPENED.lock() {
            q.extend(paths);
        }
        if let Some(ctx) = REPAINT.get() {
            ctx.request_repaint();
        }
    }

    /// Teach the app delegate (winit's) to accept `application:openURLs:`.
    fn add_open_urls_to_delegate(mtm: MainThreadMarker) {
        let app = NSApplication::sharedApplication(mtm);
        let Some(delegate) = (unsafe { app.delegate() }) else { return };
        let obj: &AnyObject = delegate.as_ref();
        let cls: &AnyClass = obj.class();
        // SAFETY: matches `-(void)application:(NSApplication*)app openURLs:(NSArray<NSURL*>*)urls`
        // ("v@:@@"); `class_addMethod` leaves the class untouched if the method already exists.
        unsafe {
            let imp: Imp = std::mem::transmute::<unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, &NSArray<NSURL>), Imp>(open_urls);
            objc2::ffi::class_addMethod(cls as *const AnyClass as *mut AnyClass, sel!(application:openURLs:), imp, c"v@:@@".as_ptr());
        }
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "DrRocketLaunchObserver"]
        #[thread_kind = MainThreadOnly]
        struct LaunchObserver;

        impl LaunchObserver {
            #[unsafe(method(willFinishLaunching:))]
            fn will_finish_launching(&self, _note: &NSNotification) {
                add_open_urls_to_delegate(self.mtm());
            }
        }
    );

    /// Must run before the event loop starts: macOS delivers documents during launch.
    pub fn install() {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let observer: Retained<LaunchObserver> = unsafe { msg_send![LaunchObserver::alloc(mtm), init] };
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &observer,
                sel!(willFinishLaunching:),
                Some(NSApplicationWillFinishLaunchingNotification),
                None,
            );
        }
        // The observer lives for the whole process.
        std::mem::forget(observer);
    }
}

/// Install the Finder open-file hook. Call from `main` before starting the event loop.
#[cfg(target_os = "macos")]
pub fn install_open_handler() {
    open_files::install();
}

#[cfg(not(target_os = "macos"))]
pub fn install_open_handler() {}
