//! Processes: collection, safety classification, human-readable descriptions and control.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};

use crate::{tr, trf};

/// How dangerous it is to stop a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Safety {
    /// A regular process of the current user.
    User,
    /// A system process or another user's process: needs explicit confirmation.
    System,
    /// MacPilot itself: quit it by closing the window.
    Own,
    /// Vital to macOS: stopping it is blocked.
    Critical,
}

impl Safety {
    pub fn label(self) -> &'static str {
        match self {
            Safety::User => tr("yours"),
            Safety::System => tr("system"),
            Safety::Critical => tr("critical"),
            Safety::Own => "MacPilot",
        }
    }

    pub fn explain(self) -> &'static str {
        match self {
            Safety::User => tr("Your process. Safe to stop; unsaved work in it may be lost."),
            Safety::System => {
                tr("System or another user's process. macOS usually restarts it, but stopping it may cause glitches. Type “yes” to confirm.")
            }
            Safety::Critical => tr("Vital to macOS. Stopping it would freeze the Mac, log you out or restart it — blocked."),
            Safety::Own => tr("This is MacPilot itself. To quit it, close the window or press ⌘Q."),
        }
    }

    /// Stopping is not offered.
    pub fn blocked(self) -> bool {
        matches!(self, Safety::Critical | Safety::Own)
    }
}

#[derive(Clone, Debug)]
pub struct ProcInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub exe: Option<PathBuf>,
    pub cmd: String,
    pub cwd: Option<PathBuf>,
    pub user: String,
    pub uid: Option<u32>,
    pub cpu: f32,
    pub mem: u64,
    pub vmem: u64,
    pub status: &'static str,
    pub stopped: bool,
    pub run_time: u64,
    pub disk_read: u64,
    pub disk_write: u64,
    pub safety: Safety,
    /// The outermost .app bundle the process belongs to.
    pub app: Option<PathBuf>,
    /// The app's main process (can be quit like ⌘Q).
    pub is_main_app: bool,
}

impl ProcInfo {
    pub fn app_name(&self) -> Option<String> {
        self.app.as_ref().map(|a| bundle_name(a))
    }
}

pub fn bundle_name(app: &Path) -> String {
    app.file_name().map(|n| n.to_string_lossy().trim_end_matches(".app").to_string()).unwrap_or_default()
}

/// Processes of one app (all Chrome helpers → "Google Chrome").
#[derive(Clone, Debug)]
pub struct AppGroup {
    pub key: String,
    pub label: String,
    pub app: Option<PathBuf>,
    pub pids: Vec<u32>,
    pub cpu: f32,
    pub mem: u64,
    pub safety: Safety,
}

/// A cheap-to-clone picture of the system, safe to pass between threads.
#[derive(Clone, Default)]
pub struct Snapshot {
    pub my_uid: u32,
    pub my_pid: u32,
    pub procs: Vec<ProcInfo>,
    pub by_pid: HashMap<u32, usize>,
    pub cpu_total: f32,
    pub cpu_count: usize,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    /// macOS memory pressure: 1 normal, 2 warning, 4 critical.
    pub pressure: u32,
}

pub struct Monitor {
    sys: System,
    users: Users,
    pub snap: Snapshot,
}

impl std::ops::Deref for Monitor {
    type Target = Snapshot;
    fn deref(&self) -> &Snapshot {
        &self.snap
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        let mut m = Monitor {
            sys: System::new(),
            users: Users::new_with_refreshed_list(),
            snap: Snapshot { my_uid: unsafe { libc::getuid() }, my_pid: std::process::id(), ..Default::default() },
        };
        m.refresh();
        m
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage()
                .with_exe(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_cwd(UpdateKind::OnlyIfNotSet)
                .with_user(UpdateKind::OnlyIfNotSet),
        );
        let (my_uid, my_pid) = (self.snap.my_uid, self.snap.my_pid);
        let mut procs = Vec::with_capacity(self.sys.processes().len());
        for (pid, p) in self.sys.processes() {
            if p.thread_kind().is_some() {
                continue;
            }
            let pid = pid.as_u32();
            let name = p.name().to_string_lossy().to_string();
            let exe = p.exe().map(|e| e.to_path_buf());
            let uid = p.user_id().map(|u| **u);
            let user = p
                .user_id()
                .and_then(|u| self.users.get_user_by_id(u))
                .map(|u| u.name().to_string())
                .unwrap_or_else(|| uid.map(|u| u.to_string()).unwrap_or_else(|| "?".into()));
            let cmd = p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" ");
            let status = p.status();
            let du = p.disk_usage();
            procs.push(ProcInfo {
                pid,
                ppid: p.parent().map(|p| p.as_u32()),
                safety: classify(pid, &name, exe.as_deref(), uid, my_uid, my_pid),
                app: exe.as_deref().and_then(outer_app),
                is_main_app: exe.as_deref().is_some_and(is_main_app_exe),
                name,
                exe,
                cmd,
                cwd: p.cwd().map(|c| c.to_path_buf()),
                user,
                uid,
                cpu: p.cpu_usage(),
                mem: p.memory(),
                vmem: p.virtual_memory(),
                status: status_text(status),
                stopped: matches!(status, sysinfo::ProcessStatus::Stop),
                run_time: p.run_time(),
                disk_read: du.read_bytes,
                disk_write: du.written_bytes,
            });
        }
        let s = &mut self.snap;
        s.by_pid = procs.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
        s.procs = procs;
        s.cpu_total = self.sys.global_cpu_usage();
        s.cpu_count = self.sys.cpus().len();
        s.mem_used = self.sys.used_memory();
        s.mem_total = self.sys.total_memory();
        s.swap_used = self.sys.used_swap();
        s.swap_total = self.sys.total_swap();
        s.pressure = memory_pressure();
    }
}

fn memory_pressure() -> u32 {
    let mut v: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>();
    let name = c"kern.memorystatus_vm_pressure_level";
    let r = unsafe { libc::sysctlbyname(name.as_ptr(), &mut v as *mut _ as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) };
    if r == 0 { v as u32 } else { 1 }
}

impl Snapshot {
    pub fn is_root(&self) -> bool {
        self.my_uid == 0
    }

    pub fn get(&self, pid: u32) -> Option<&ProcInfo> {
        self.by_pid.get(&pid).map(|&i| &self.procs[i])
    }

    pub fn alive(&self, pid: u32) -> bool {
        self.by_pid.contains_key(&pid)
    }

    pub fn cpu_total(&self) -> f32 {
        self.cpu_total
    }
    pub fn cpu_count(&self) -> usize {
        self.cpu_count
    }
    pub fn mem_used(&self) -> u64 {
        self.mem_used
    }
    pub fn mem_total(&self) -> u64 {
        self.mem_total
    }
    pub fn swap_used(&self) -> u64 {
        self.swap_used
    }
    pub fn swap_total(&self) -> u64 {
        self.swap_total
    }

    /// Group processes by app.
    pub fn groups(&self) -> Vec<AppGroup> {
        let mut map: HashMap<String, AppGroup> = HashMap::new();
        for p in &self.procs {
            let (key, label) = match &p.app {
                Some(app) => (app.to_string_lossy().to_string(), bundle_name(app)),
                None => (format!("name:{}", p.name), p.name.clone()),
            };
            let g = map.entry(key.clone()).or_insert_with(|| AppGroup {
                key,
                label,
                app: p.app.clone(),
                pids: Vec::new(),
                cpu: 0.0,
                mem: 0,
                safety: Safety::User,
            });
            g.pids.push(p.pid);
            g.cpu += p.cpu;
            g.mem += p.mem;
            g.safety = g.safety.max(p.safety);
        }
        map.into_values().collect()
    }

    /// Depth-first order of the process tree: (pid, depth).
    pub fn tree_order(&self, visible: &HashSet<u32>, cmp: &dyn Fn(&ProcInfo, &ProcInfo) -> std::cmp::Ordering) -> Vec<(u32, usize)> {
        let mut children: HashMap<Option<u32>, Vec<u32>> = HashMap::new();
        for p in &self.procs {
            if !visible.contains(&p.pid) {
                continue;
            }
            // A root is a process whose parent is hidden or gone.
            let parent = p.ppid.filter(|pp| visible.contains(pp) && *pp != p.pid);
            children.entry(parent).or_default().push(p.pid);
        }
        for v in children.values_mut() {
            v.sort_by(|a, b| cmp(self.get(*a).unwrap(), self.get(*b).unwrap()));
        }
        let mut out = Vec::with_capacity(visible.len());
        let mut stack: Vec<(u32, usize)> = children.get(&None).map(|v| v.iter().rev().map(|p| (*p, 0)).collect()).unwrap_or_default();
        let mut seen = HashSet::new();
        while let Some((pid, depth)) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            out.push((pid, depth));
            if let Some(ch) = children.get(&Some(pid)) {
                for c in ch.iter().rev() {
                    stack.push((*c, depth + 1));
                }
            }
        }
        out
    }
}

fn status_text(s: sysinfo::ProcessStatus) -> &'static str {
    use sysinfo::ProcessStatus::*;
    match s {
        Run => tr("running"),
        Sleep => tr("waiting"),
        Idle => tr("idle"),
        Stop => tr("PAUSED"),
        Zombie => tr("zombie"),
        _ => "—",
    }
}

/// The outermost `.app` in a path (Chrome.app for helpers inside it).
pub fn outer_app(exe: &Path) -> Option<PathBuf> {
    let mut acc = PathBuf::new();
    for c in exe.components() {
        acc.push(c);
        if c.as_os_str().to_string_lossy().ends_with(".app") {
            return Some(acc);
        }
    }
    None
}

/// `X.app/Contents/MacOS/X` at the top level of a bundle — the app's main executable.
fn is_main_app_exe(exe: &Path) -> bool {
    let Some(app) = outer_app(exe) else { return false };
    exe.parent() == Some(&app.join("Contents").join("MacOS"))
}

/// Stopping these would freeze the Mac, log you out or restart it.
const CRITICAL: &[&str] = &[
    "kernel_task",
    "launchd",
    "WindowServer",
    "loginwindow",
    "logd",
    "configd",
    "securityd",
    "opendirectoryd",
    "diskarbitrationd",
    "fseventsd",
    "notifyd",
    "powerd",
    "watchdogd",
    "kextd",
    "kernelmanagerd",
    "coreservicesd",
    "launchservicesd",
    "runningboardd",
    "trustd",
    "syspolicyd",
    "amfid",
    "endpointsecurityd",
    "sandboxd",
    "distnoted",
    "UserEventAgent",
    "thermalmonitord",
    "hidd",
    "apfsd",
    "mDNSResponder",
    "tccd",
    "cfprefsd",
    "backboardd",
];

pub fn classify(pid: u32, name: &str, exe: Option<&Path>, uid: Option<u32>, my_uid: u32, my_pid: u32) -> Safety {
    if pid == my_pid {
        return Safety::Own;
    }
    if pid <= 1 || CRITICAL.contains(&name) {
        return Safety::Critical;
    }
    let sys_path = exe.is_some_and(|e| {
        let s = e.to_string_lossy();
        s.starts_with("/System/") || s.starts_with("/usr/libexec/") || s.starts_with("/usr/sbin/") || s.starts_with("/sbin/")
    });
    if sys_path || uid != Some(my_uid) {
        return Safety::System;
    }
    Safety::User
}

/// What the process is, in plain words.
pub fn describe(p: &ProcInfo) -> String {
    let exe_name = p.exe.as_ref().and_then(|e| e.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    for n in [p.name.as_str(), exe_name.as_str()] {
        if let Some(d) = known(n) {
            return d.to_string();
        }
    }
    let exe_s = p.exe.as_ref().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
    if let Some(app) = p.app_name() {
        if p.is_main_app {
            return if exe_s.starts_with("/System/") {
                trf("Built-in macOS app “{0}” — main process.", &[&app])
            } else {
                trf("App “{0}” — main process.", &[&app])
            };
        }
        return trf("Helper process of “{0}” (tabs, rendering, plug-ins and so on).", &[&app]);
    }
    let s = if exe_s.starts_with("/System/") || exe_s.starts_with("/usr/libexec/") || exe_s.starts_with("/usr/sbin/") {
        tr("A macOS background service. launchd starts it and restarts it if needed.")
    } else if exe_s.starts_with("/opt/homebrew/") || exe_s.starts_with("/usr/local/") {
        tr("A program installed with Homebrew or by hand.")
    } else if exe_s.starts_with("/usr/bin/") || exe_s.starts_with("/bin/") {
        tr("A standard macOS command-line tool.")
    } else if exe_s.contains("/.cargo/") {
        tr("A tool installed with Cargo (Rust).")
    } else if exe_s.contains("/.nvm/") || exe_s.ends_with("/node") {
        tr("A Node.js process.")
    } else if exe_s.is_empty() {
        tr("Path not available (usually a root process — macOS hides its details).")
    } else {
        tr("Unknown process — check its path and command line below.")
    };
    s.to_string()
}

fn known(name: &str) -> Option<&'static str> {
    Some(match name {
        "kernel_task" => tr("The macOS kernel. High CPU often means the Mac is hot and the kernel is throttling to cool it down."),
        "launchd" => tr("PID 1 — the parent of all processes. Starts services and watches them."),
        "WindowServer" => tr("Draws everything on screen. Uses more CPU with several displays, transparency and animations."),
        "loginwindow" => tr("Your login session. Stopping it logs you out."),
        "Finder" => tr("Finder: files and the desktop. Safe to relaunch."),
        "Dock" => tr("Dock, Mission Control and Launchpad. Restarts automatically."),
        "SystemUIServer" | "ControlCenter" => tr("Menu bar icons and Control Center."),
        "NotificationCenter" => tr("Notifications and widgets."),
        "WindowManager" => tr("Window management and Stage Manager."),
        "mds" | "mds_stores" | "mdworker" | "mdworker_shared" | "mdbulkimport" | "corespotlightd" => {
            tr("Spotlight indexing. Heavy use after updates or copying many files is normal and passes.")
        }
        "backupd" | "backupd-helper" => tr("Time Machine backups."),
        "cloudd" | "bird" | "fileproviderd" => tr("iCloud and cloud drives syncing (iCloud Drive, Dropbox, Google Drive…)."),
        "photoanalysisd" | "mediaanalysisd" => tr("Photo and video analysis (faces, objects, Live Text). Runs while the Mac is idle."),
        "photolibraryd" => tr("The Photos library."),
        "softwareupdated" => tr("Checks for and downloads macOS updates."),
        "nsurlsessiond" => tr("Background downloads for macOS and apps."),
        "trustd" | "securityd" | "secd" => tr("Certificates, keychain and encryption."),
        "syspolicyd" | "XprotectService" | "XProtect" | "XProtectPluginService" | "amfid" => {
            tr("Built-in security: Gatekeeper, XProtect antivirus and code signature checks.")
        }
        "coreaudiod" => tr("Sound. If audio stops working, restarting it often helps (it relaunches itself)."),
        "bluetoothd" => tr("Bluetooth."),
        "airportd" | "WiFiAgent" => tr("Wi-Fi."),
        "mDNSResponder" => tr("DNS and Bonjour. Without it the internet does not work."),
        "configd" => tr("Network configuration."),
        "powerd" => tr("Power and sleep."),
        "hidd" => tr("Keyboard, mouse and trackpad."),
        "corebrightnessd" => tr("Display brightness and Night Shift."),
        "runningboardd" => tr("Manages the lifecycle of app processes."),
        "cfprefsd" => tr("Stores app settings (plist files)."),
        "logd" => tr("System log."),
        "fseventsd" => tr("Tracks file changes (for Spotlight, Time Machine, IDEs)."),
        "sharingd" | "rapportd" => tr("AirDrop, Handoff and Continuity with your other Apple devices."),
        "identityservicesd" | "imagent" => tr("iMessage and FaceTime."),
        "suggestd" | "assistantd" | "Siri" | "knowledge-agent" | "biomed" | "duetexpertd" => tr("Siri and suggestions."),
        "akd" | "accountsd" => tr("Apple ID and internet accounts."),
        "tccd" => tr("Privacy permissions (camera, microphone, file access)."),
        "locationd" => tr("Location services."),
        "com.apple.WebKit.WebContent" => tr("A Safari tab or a web view inside an app. Many of them are normal."),
        "com.apple.WebKit.Networking" | "com.apple.WebKit.GPU" => tr("WebKit helper (Safari and web views)."),
        "node" => tr("Node.js — a script or a dev server (npm, vite, next…)."),
        "python" | "python3" | "Python" => tr("A Python script."),
        "ruby" => tr("A Ruby script."),
        "java" => tr("A Java app (Gradle, IntelliJ, Minecraft…)."),
        "com.docker.backend" | "com.docker.virtualization" | "com.docker.build" | "Docker Desktop" => {
            tr("Docker. Its virtual machine can use a lot of memory.")
        }
        "com.apple.Virtualization.VirtualMachine" => tr("A virtual machine (Docker, UTM, OrbStack, simulators…)."),
        "zsh" | "bash" | "fish" | "sh" | "login" => tr("A terminal shell."),
        "sshd" | "sshd-session" => tr("SSH server — someone is connected to this Mac, or Remote Login is on."),
        "ssh" => tr("SSH client — a connection to a remote server."),
        "ssh-agent" | "gpg-agent" => tr("Keeps SSH/GPG keys in memory."),
        "Terminal" | "iTerm2" => tr("Terminal."),
        "cargo" | "rustc" | "rust-analyzer" => tr("Rust tools (build, compiler, editor support)."),
        "thermalmonitord" => tr("Temperature monitoring."),
        "watchdogd" => tr("Watchdog. Stopping it restarts the Mac."),
        "screencaptureui" => tr("Screenshots and screen recording."),
        "ReportCrash" | "spindump" => tr("Collects crash and hang reports."),
        "VTDecoderXPCService" => tr("Hardware video decoding."),
        "MTLCompilerService" => tr("Compiles Metal shaders (graphics)."),
        "usbmuxd" | "AMPDevicesAgent" => tr("Talks to your iPhone or iPad over USB."),
        "postgres" | "mysqld" | "redis-server" | "mongod" => tr("A database server (usually for development)."),
        "nginx" | "httpd" => tr("A web server."),
        "ollama" => tr("Ollama — local AI models (can use a lot of memory and GPU)."),
        "SourceKitService" | "XCBBuildService" => tr("Xcode helper (code completion, building)."),
        "macpilot" | "macpilot-gui" | "MacPilot" => tr("This app (MacPilot)."),
        _ => return None,
    })
}

/// Send a signal. Returns a readable error.
pub fn send_signal(pid: u32, sig: i32) -> Result<(), String> {
    if unsafe { libc::kill(pid as libc::pid_t, sig) } == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    Err(match err.raw_os_error() {
        Some(libc::EPERM) => tr("not allowed — the process belongs to another user or root").into(),
        Some(libc::ESRCH) => tr("the process has already exited").into(),
        _ => err.to_string(),
    })
}

/// Send a signal as administrator: macOS shows its own password prompt.
pub fn send_signal_admin(pids: &[u32], sig: i32) -> Result<(), String> {
    let list = pids.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(" ");
    let script = format!("do shell script \"/bin/kill -{sig} {list}\" with administrator privileges");
    let out = std::process::Command::new("osascript").arg("-e").arg(script).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    Err(if err.contains("-128") { tr("Cancelled.").into() } else { err.trim().to_string() })
}

/// Quit an app like ⌘Q — it can save its state and ask about unsaved documents.
/// Runs in the background because the app may show a save dialog.
pub fn quit_app(app: &Path) {
    let path = app.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("tell application \"{path}\" to quit");
    std::thread::spawn(move || {
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// Listening network ports per process (TCP listen + UDP), via `lsof`.
pub fn listening_ports() -> HashMap<u32, Vec<String>> {
    let out = crate::run_output("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN", "-iUDP", "-F", "pPn"]);
    let mut m: HashMap<u32, Vec<String>> = HashMap::new();
    let (mut pid, mut proto) = (0u32, String::new());
    for line in out.lines() {
        let (tag, val) = line.split_at(1);
        match tag {
            "p" => pid = val.parse().unwrap_or(0),
            "P" => proto = val.to_string(),
            "n" if pid > 0 => {
                // "*:3000", "127.0.0.1:5432", "[::1]:8080"; skip connected UDP sockets ("a->b").
                if val.contains("->") {
                    continue;
                }
                let port = val.rsplit(':').next().unwrap_or("");
                if port.is_empty() || port == "*" {
                    continue;
                }
                let local = val.starts_with("127.") || val.starts_with("[::1]") || val.starts_with("localhost");
                let entry = if local { format!("{proto} {port} ({})", tr("local")) } else { format!("{proto} {port}") };
                let v = m.entry(pid).or_default();
                if !v.contains(&entry) {
                    v.push(entry);
                }
            }
            _ => {}
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_processes() {
        assert_eq!(classify(1, "launchd", None, Some(0), 501, 9999), Safety::Critical);
        assert_eq!(classify(300, "WindowServer", None, Some(88), 501, 9999), Safety::Critical);
        assert_eq!(classify(9999, "macpilot", None, Some(501), 501, 9999), Safety::Own);
        assert_eq!(classify(500, "mds", Some(Path::new("/System/Library/mds")), Some(0), 501, 1), Safety::System);
        assert_eq!(classify(600, "node", Some(Path::new("/opt/homebrew/bin/node")), Some(501), 501, 1), Safety::User);
    }

    #[test]
    fn app_bundles() {
        let helper = Path::new(
            "/Applications/Google Chrome.app/Contents/Frameworks/X.framework/Helpers/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper",
        );
        assert_eq!(outer_app(helper).unwrap(), Path::new("/Applications/Google Chrome.app"));
        assert!(!is_main_app_exe(helper));
        assert!(is_main_app_exe(Path::new("/Applications/Safari.app/Contents/MacOS/Safari")));
    }
}
