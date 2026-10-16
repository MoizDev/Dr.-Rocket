//! Platform window chrome.
//!
//! On macOS the window uses a transparent, full-size-content title bar; an empty unified
//! toolbar makes the title bar taller so the traffic lights sit vertically centered in
//! Racketeer's own title bar (the same trick native "unified toolbar" apps use).

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
