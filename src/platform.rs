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
    if std::env::var_os("DRROCKET_TRACE").is_some()
        && let Some(close) = window.standardWindowButton(objc2_app_kit::NSWindowButton::CloseButton)
        && let Some(sup) = unsafe { close.superview() }
    {
        // Report where macOS put the traffic lights, measured from the window top.
        let f = close.frame();
        let in_window = sup.convertRect_toView(f, None);
        let win_h = window.contentLayoutRect().size.height + (window.frame().size.height - window.contentLayoutRect().size.height);
        let center_from_top = win_h - (in_window.origin.y + in_window.size.height / 2.0);
        eprintln!("traffic lights: x={:.1} center_from_top={:.1} (title bar center {:.1})", in_window.origin.x, center_from_top, NATIVE_TITLEBAR_H / 2.0);
    }
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
        let Some(delegate) = app.delegate() else { return };
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

// ---------------------------------------------------------------------------------------
// Native menu bar (macOS).
//
// Replaces winit's default menu, whose Quit item terminates immediately without asking about
// unsaved changes. Menu items post actions to a queue that the app drains every frame.

/// What a menu item asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// Index into `commands::COMMANDS`.
    Command(usize),
    /// Quit, going through the unsaved-changes prompt.
    Quit,
}

static MENU_ACTIONS: std::sync::Mutex<Vec<MenuAction>> = std::sync::Mutex::new(Vec::new());

/// Menu actions chosen since the last call.
pub fn take_menu_actions() -> Vec<MenuAction> {
    MENU_ACTIONS.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

/// One entry of a menu: label, action, and an optional key equivalent.
pub struct MenuEntry {
    pub label: String,
    pub action: MenuAction,
    pub shortcut: Option<egui::KeyboardShortcut>,
}

/// A titled menu; `None` entries are separators.
pub struct MenuSpec {
    pub title: &'static str,
    pub entries: Vec<Option<MenuEntry>>,
}

/// Disable winit's default menu (Quit there skips our unsaved-changes prompt).
pub fn event_loop_hook() -> Option<eframe::EventLoopBuilderHook> {
    #[cfg(target_os = "macos")]
    {
        Some(Box::new(|builder| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            builder.with_default_menu(false);
        }))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
mod menu {
    use super::{MENU_ACTIONS, MenuAction, MenuSpec, REPAINT};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject, Sel};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem};
    use objc2_foundation::NSString;

    /// Encode actions as menu item tags: commands are >= 0, Quit is -1.
    fn tag_of(a: MenuAction) -> isize {
        match a {
            MenuAction::Command(i) => i as isize,
            MenuAction::Quit => -1,
        }
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "DrRocketMenuTarget"]
        #[thread_kind = MainThreadOnly]
        struct MenuTarget;

        impl MenuTarget {
            #[unsafe(method(menuAction:))]
            fn menu_action(&self, sender: &NSMenuItem) {
                let tag = sender.tag();
                let action = if tag < 0 { MenuAction::Quit } else { MenuAction::Command(tag as usize) };
                if let Ok(mut q) = MENU_ACTIONS.lock() {
                    q.push(action);
                }
                if let Some(ctx) = REPAINT.get() {
                    ctx.request_repaint();
                }
            }
        }
    );

    fn key_equivalent(s: &egui::KeyboardShortcut) -> Option<(String, NSEventModifierFlags)> {
        use egui::Key;
        let key = match s.logical_key {
            Key::ArrowLeft => "\u{F702}".to_string(),
            Key::ArrowRight => "\u{F703}".to_string(),
            Key::ArrowUp => "\u{F700}".to_string(),
            Key::ArrowDown => "\u{F701}".to_string(),
            Key::F1 => "\u{F704}".to_string(),
            Key::F2 => "\u{F705}".to_string(),
            Key::F8 => "\u{F70B}".to_string(),
            Key::F12 => "\u{F70F}".to_string(),
            Key::Period => ".".into(),
            Key::Comma => ",".into(),
            Key::Equals => "=".into(),
            Key::Minus => "-".into(),
            Key::Backtick => "`".into(),
            other => {
                let n = other.name();
                if n.chars().count() == 1 { n.to_lowercase() } else { return None }
            }
        };
        let m = s.modifiers;
        let mut flags = NSEventModifierFlags::empty();
        if m.command || m.mac_cmd {
            flags |= NSEventModifierFlags::Command;
        }
        if m.shift {
            flags |= NSEventModifierFlags::Shift;
        }
        if m.alt {
            flags |= NSEventModifierFlags::Option;
        }
        if m.ctrl {
            flags |= NSEventModifierFlags::Control;
        }
        Some((key, flags))
    }

    fn item(mtm: MainThreadMarker, title: &str, action: Option<Sel>, key: &str, mods: NSEventModifierFlags) -> Retained<NSMenuItem> {
        let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), action, &NSString::from_str(key)) };
        if !key.is_empty() {
            item.setKeyEquivalentModifierMask(mods);
        }
        item
    }

    fn submenu(mtm: MainThreadMarker, title: &str, menu: &NSMenu) -> Retained<NSMenuItem> {
        let it = item(mtm, title, None, "", NSEventModifierFlags::empty());
        it.setSubmenu(Some(menu));
        it
    }

    pub fn install(app_name: &str, specs: &[MenuSpec]) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target: Retained<MenuTarget> = unsafe { msg_send![MenuTarget::alloc(mtm), init] };
        let target_obj: &AnyObject = target.as_ref();
        let app = NSApplication::sharedApplication(mtm);
        let main = NSMenu::new(mtm);
        let cmd = NSEventModifierFlags::Command;

        let add_entries = |menu: &NSMenu, entries: &[Option<super::MenuEntry>]| {
            for e in entries {
                match e {
                    None => menu.addItem(&NSMenuItem::separatorItem(mtm)),
                    Some(e) => {
                        let (key, mods) = e.shortcut.as_ref().and_then(key_equivalent).unwrap_or_else(|| (String::new(), NSEventModifierFlags::empty()));
                        let it = item(mtm, &e.label, Some(sel!(menuAction:)), &key, mods);
                        unsafe { it.setTarget(Some(target_obj)) };
                        it.setTag(tag_of(e.action));
                        menu.addItem(&it);
                    }
                }
            }
        };

        // Application menu.
        let app_menu = NSMenu::new(mtm);
        for spec in specs.iter().filter(|s| s.title == "App") {
            add_entries(&app_menu, &spec.entries);
        }
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        let services = NSMenu::new(mtm);
        app_menu.addItem(&submenu(mtm, "Services", &services));
        app.setServicesMenu(Some(&services));
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        app_menu.addItem(&item(mtm, &format!("Hide {app_name}"), Some(sel!(hide:)), "h", cmd));
        app_menu.addItem(&item(mtm, "Hide Others", Some(sel!(hideOtherApplications:)), "h", cmd | NSEventModifierFlags::Option));
        app_menu.addItem(&item(mtm, "Show All", Some(sel!(unhideAllApplications:)), "", NSEventModifierFlags::empty()));
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        let quit = item(mtm, &format!("Quit {app_name}"), Some(sel!(menuAction:)), "q", cmd);
        unsafe { quit.setTarget(Some(target_obj)) };
        quit.setTag(tag_of(MenuAction::Quit));
        app_menu.addItem(&quit);
        main.addItem(&submenu(mtm, app_name, &app_menu));

        for spec in specs.iter().filter(|s| s.title != "App") {
            let m = unsafe { NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(spec.title)) };
            add_entries(&m, &spec.entries);
            main.addItem(&submenu(mtm, spec.title, &m));
            if spec.title == "Window" {
                m.addItem(&NSMenuItem::separatorItem(mtm));
                m.addItem(&item(mtm, "Minimize", Some(sel!(performMiniaturize:)), "m", cmd));
                m.addItem(&item(mtm, "Zoom", Some(sel!(performZoom:)), "", NSEventModifierFlags::empty()));
                app.setWindowsMenu(Some(&m));
            }
        }
        app.setMainMenu(Some(&main));
        // Menu items keep a weak reference to their target; keep it alive for the process.
        std::mem::forget(target);
    }
}

/// Invoke a menu item by menu and item title, as a click would (used by automation).
pub fn perform_menu_item(menu: &str, item: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        use objc2_foundation::NSString;
        let Some(mtm) = MainThreadMarker::new() else { return false };
        let app = NSApplication::sharedApplication(mtm);
        let Some(main) = app.mainMenu() else { return false };
        let Some(top) = main.itemWithTitle(&NSString::from_str(menu)) else { return false };
        let Some(sub) = top.submenu() else { return false };
        let idx = sub.indexOfItemWithTitle(&NSString::from_str(item));
        if idx < 0 {
            return false;
        }
        sub.performActionForItemAtIndex(idx);
        true
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (menu, item);
        false
    }
}

/// Install the native menu bar (macOS only).
pub fn install_menu(app_name: &str, specs: &[MenuSpec]) {
    #[cfg(target_os = "macos")]
    menu::install(app_name, specs);
    #[cfg(not(target_os = "macos"))]
    let _ = (app_name, specs);
}
