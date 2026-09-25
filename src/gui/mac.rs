//! Native macOS pieces egui does not cover: the menu bar item, hiding the window to the menu bar,
//! and launch at login through `SMAppService`. Everything here runs on the main thread.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem};
use objc2_foundation::NSString;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

define_class!(
    // Receives clicks on the menu bar menu.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MacPilotMenuTarget"]
    struct MenuTarget;

    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        #[unsafe(method(openWindow:))]
        fn open_window(&self, _sender: Option<&AnyObject>) {
            show_window();
        }

        #[unsafe(method(quitApp:))]
        fn quit_app(&self, _sender: Option<&AnyObject>) {
            if let Some(mtm) = MainThreadMarker::new() {
                NSApplication::sharedApplication(mtm).terminate(None);
            }
        }
    }
);

struct StatusMenu {
    item: Retained<NSStatusItem>,
    /// Read-only lines at the top of the menu (CPU, memory, disk…).
    lines: Vec<Retained<NSMenuItem>>,
    open: Retained<NSMenuItem>,
    quit: Retained<NSMenuItem>,
    _target: Retained<MenuTarget>,
    title: String,
}

thread_local! {
    static STATUS: RefCell<Option<StatusMenu>> = const { RefCell::new(None) };
}

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

fn menu_item(mtm: MainThreadMarker, title: &str, action: Option<objc2::runtime::Sel>, key: &str) -> Retained<NSMenuItem> {
    unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &ns(title), action, &ns(key)) }
}

/// What the menu bar item shows.
pub struct StatusInfo<'a> {
    /// Short text next to the icon, e.g. "23% · 81%".
    pub title: &'a str,
    pub lines: &'a [String],
    pub open_label: &'a str,
    pub quit_label: &'a str,
}

/// Show or update the menu bar item; `None` removes it.
pub fn set_status(info: Option<StatusInfo>) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    STATUS.with(|cell| {
        let mut cur = cell.borrow_mut();
        let Some(info) = info else {
            if let Some(s) = cur.take() {
                NSStatusBar::systemStatusBar().removeStatusItem(&s.item);
            }
            return;
        };
        // Rebuild when the number of lines changes; otherwise only retitle.
        if cur.as_ref().is_some_and(|s| s.lines.len() != info.lines.len()) {
            if let Some(s) = cur.take() {
                NSStatusBar::systemStatusBar().removeStatusItem(&s.item);
            }
        }
        if cur.is_none() {
            let item = NSStatusBar::systemStatusBar().statusItemWithLength(-1.0); // NSVariableStatusItemLength
            let target: Retained<MenuTarget> = unsafe { msg_send![MenuTarget::alloc(mtm), init] };
            let menu = NSMenu::new(mtm);
            let mut lines = Vec::new();
            for _ in info.lines {
                let l = menu_item(mtm, "", None, "");
                l.setEnabled(false);
                menu.addItem(&l);
                lines.push(l);
            }
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            let open = menu_item(mtm, info.open_label, Some(sel!(openWindow:)), "");
            let quit = menu_item(mtm, info.quit_label, Some(sel!(quitApp:)), "q");
            for i in [&open, &quit] {
                unsafe { i.setTarget(Some(&target)) };
                menu.addItem(i);
            }
            // Without this, disabled lines stay grey but items without an action would be greyed too.
            menu.setAutoenablesItems(false);
            item.setMenu(Some(&menu));
            *cur = Some(StatusMenu { item, lines, open, quit, _target: target, title: String::new() });
        }
        let s = cur.as_mut().expect("status menu");
        if s.title != info.title {
            if let Some(b) = s.item.button(mtm) {
                b.setTitle(&ns(info.title));
            }
            s.title = info.title.to_string();
        }
        for (item, text) in s.lines.iter().zip(info.lines) {
            item.setTitle(&ns(text));
        }
        s.open.setTitle(&ns(info.open_label));
        s.quit.setTitle(&ns(info.quit_label));
    });
}

/// Hide the window and the Dock icon; MacPilot keeps running in the menu bar.
pub fn hide_window() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    for w in app.windows().iter() {
        if w.canBecomeMainWindow() {
            w.orderOut(None);
        }
    }
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

/// Bring the window back, with the Dock icon.
pub fn show_window() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    for w in app.windows().iter() {
        if w.canBecomeMainWindow() {
            if w.isMiniaturized() {
                w.deminiaturize(None);
            }
            w.makeKeyAndOrderFront(None);
        }
    }
    #[allow(deprecated)] // `activate` needs macOS 14
    app.activateIgnoringOtherApps(true);
}

// ---------------------------------------------------------------------------
// Launch at login (macOS 13+)
// ---------------------------------------------------------------------------

/// `SMAppService` exists (macOS 13+) and we run from an app bundle, which it needs.
pub fn login_item_supported() -> bool {
    AnyClass::get(c"SMAppService").is_some() && std::env::current_exe().ok().and_then(|e| macpilot::procs::outer_app(&e)).is_some()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoginItem {
    On,
    /// Registered, but switched off in System Settings → Login Items.
    NeedsApproval,
    Off,
}

pub fn login_item() -> LoginItem {
    let s = unsafe { SMAppService::mainAppService().status() };
    match s {
        SMAppServiceStatus::Enabled => LoginItem::On,
        SMAppServiceStatus::RequiresApproval => LoginItem::NeedsApproval,
        _ => LoginItem::Off,
    }
}

pub fn set_login_item(on: bool) -> Result<(), String> {
    let svc = unsafe { SMAppService::mainAppService() };
    let r = unsafe { if on { svc.registerAndReturnError() } else { svc.unregisterAndReturnError() } };
    r.map_err(|e| e.localizedDescription().to_string())
}

pub fn open_login_items_settings() {
    unsafe { SMAppService::openSystemSettingsLoginItems() };
}
