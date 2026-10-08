//! Native macOS pieces egui does not cover: the menu bar item, hiding the window to the menu bar,
//! and launch at login through `SMAppService`. Everything here runs on the main thread.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSCellImagePosition, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem};
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
    /// Short text next to the icon, e.g. "CPU 23% · RAM 81%".
    pub title: &'a str,
    /// Shown on hover, so it is clear the item belongs to MacPilot.
    pub tooltip: &'a str,
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
            if let Some(b) = item.button(mtm) {
                // A template symbol (macOS 11+) tints itself for light and dark menu bars.
                if let Some(img) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns("gauge.medium"), Some(&ns("MacPilot")))
                    .or_else(|| NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns("speedometer"), Some(&ns("MacPilot"))))
                {
                    img.setTemplate(true);
                    b.setImage(Some(&img));
                    b.setImagePosition(NSCellImagePosition::ImageLeading);
                }
            }
            *cur = Some(StatusMenu { item, lines, open, quit, _target: target, title: String::new() });
        }
        let s = cur.as_mut().expect("status menu");
        if s.title != info.title {
            if let Some(b) = s.item.button(mtm) {
                b.setTitle(&ns(info.title));
            }
            s.title = info.title.to_string();
        }
        if let Some(b) = s.item.button(mtm) {
            b.setToolTip(Some(&ns(info.tooltip)));
        }
        for (item, text) in s.lines.iter().zip(info.lines) {
            item.setTitle(&ns(text));
        }
        s.open.setTitle(&ns(info.open_label));
        s.quit.setTitle(&ns(info.quit_label));
    });
}

/// Quit the app (as ⌘Q does).
pub fn quit() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).terminate(None);
    }
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

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

use objc2::runtime::ProtocolObject;
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions, UNNotificationRequest,
    UNNotificationResponse, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

define_class!(
    // Shows banners while MacPilot is in front and opens the right page on a click.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MacPilotNotificationDelegate"]
    struct NotifyDelegate;

    unsafe impl NSObjectProtocol for NotifyDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotifyDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _c: &UNUserNotificationCenter,
            _n: &UNNotification,
            done: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            done.call((UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(&self, _c: &UNUserNotificationCenter, r: &UNNotificationResponse, done: &block2::DynBlock<dyn Fn()>) {
            let id = r.notification().request().identifier().to_string();
            *CLICKED.lock().unwrap() = Some(id);
            show_window();
            done.call(());
        }
    }
);

static CLICKED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

thread_local! {
    static NOTIFY: RefCell<Option<Retained<NotifyDelegate>>> = const { RefCell::new(None) };
}

/// Notifications need a real app bundle (the notification center refuses a bare binary).
fn can_notify() -> bool {
    std::env::current_exe().ok().and_then(|e| macpilot::procs::outer_app(&e)).is_some()
}

/// Start listening for clicks on notifications. Permission is asked later, with the first real one.
pub fn init_notifications() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    if !can_notify() || NOTIFY.with(|n| n.borrow().is_some()) {
        return;
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let delegate: Retained<NotifyDelegate> = unsafe { msg_send![NotifyDelegate::alloc(mtm), init] };
    center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    NOTIFY.with(|n| *n.borrow_mut() = Some(delegate));
}

/// Show a notification. The same `id` replaces the previous one instead of stacking up.
/// The first one asks macOS for permission (so the question comes with a reason) and is sent once allowed.
pub fn notify(id: &str, title: &str, body: &str) {
    if !can_notify() || NOTIFY.with(|n| n.borrow().is_none()) {
        return;
    }
    let (id, title, body) = (id.to_string(), title.to_string(), body.to_string());
    let done = block2::RcBlock::new(move |granted: objc2::runtime::Bool, _err: *mut objc2_foundation::NSError| {
        if !granted.as_bool() {
            return;
        }
        let content = UNMutableNotificationContent::new();
        content.setTitle(&ns(&title));
        content.setBody(&ns(&body));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&ns(&id), &content, None);
        UNUserNotificationCenter::currentNotificationCenter().addNotificationRequest_withCompletionHandler(&request, None);
    });
    // Asks only the first time; afterwards it answers at once with the user's choice.
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound, &done);
}

/// The id of a notification the user clicked since the last call.
pub fn take_clicked() -> Option<String> {
    CLICKED.lock().unwrap().take()
}

// ---------------------------------------------------------------------------
// Choosing folders
// ---------------------------------------------------------------------------

/// The standard "choose a folder" dialog; returns the chosen folders (empty when cancelled).
pub fn choose_folders(prompt: &str, message: &str) -> Vec<std::path::PathBuf> {
    let Some(mtm) = MainThreadMarker::new() else { return Vec::new() };
    let panel = objc2_app_kit::NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setAllowsMultipleSelection(true);
    panel.setCanCreateDirectories(false);
    panel.setPrompt(Some(&ns(prompt)));
    panel.setMessage(Some(&ns(message)));
    if panel.runModal() != objc2_app_kit::NSModalResponseOK {
        return Vec::new();
    }
    panel.URLs().iter().filter_map(|u| u.path()).map(|p| std::path::PathBuf::from(p.to_string())).collect()
}

// ---------------------------------------------------------------------------
// File icons
// ---------------------------------------------------------------------------

/// The Finder icon of `path` (of a Unix executable when `None`), drawn at `px`×`px` pixels.
/// Premultiplied RGBA, top row first.
pub fn icon_rgba(path: Option<&std::path::Path>, px: usize) -> Option<Vec<u8>> {
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext, NSWorkspace};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    MainThreadMarker::new()?;
    let ws = NSWorkspace::sharedWorkspace();
    let image = match path {
        Some(p) => ws.iconForFile(&ns(p.to_str()?)),
        // Any Unix executable has the generic "exec" icon.
        None => ws.iconForFile(&ns("/bin/sh")),
    };
    let n = px as isize;
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            n,
            n,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            n * 4,
            32,
        )
    }?;
    let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&ctx));
    image.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(px as f64, px as f64)));
    ctx.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();
    let data = rep.bitmapData();
    if data.is_null() {
        return None;
    }
    let row = rep.bytesPerRow() as usize;
    let mut out = Vec::with_capacity(px * px * 4);
    for y in 0..px {
        // SAFETY: the bitmap has `px` rows of `row` >= px * 4 bytes and lives as long as `rep`.
        out.extend_from_slice(unsafe { std::slice::from_raw_parts(data.add(y * row), px * 4) });
    }
    Some(out)
}
