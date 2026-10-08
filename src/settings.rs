//! User settings, stored as simple `key = value` lines in
//! `~/Library/Application Support/MacPilot/settings.conf`.

use std::path::PathBuf;

use crate::i18n::Lang;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// Look of the app.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UiStyle {
    /// The standard look.
    Standard,
    /// Black and white like the first Macintosh: 1-pixel lines, square corners, pixel font.
    Classic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `None` = follow the macOS language.
    pub lang: Option<Lang>,
    pub theme: Theme,
    pub style: UiStyle,
    /// "Not used for" threshold of the Stale view, days.
    pub stale_days: i64,
    /// Build folders of projects untouched for this many days are recommended for removal.
    pub junk_days: i64,
    /// Files smaller than this are ignored by the duplicate finder, MB.
    pub dupes_min_mb: u64,
    /// Scan the home folder automatically when the app starts.
    pub scan_on_start: bool,
    /// Show CPU and memory in the menu bar; closing the window keeps MacPilot running there.
    pub menu_bar: bool,
    /// Look for a newer release on GitHub once a day.
    pub check_updates: bool,
    /// macOS notifications about problems (a stuck app, a full disk, the battery).
    pub notifications: bool,
    /// Clean caches and logs by itself every this many days (0 = never).
    pub auto_clean_days: i64,
    /// When the scheduled cleanup last ran (unix seconds).
    pub auto_clean_last: i64,
    /// The user agreed that MacPilot reads the home folder (Disk, Cleanup). Asked on first use.
    pub file_access: bool,
    /// Folders MacPilot never opens: any folder, "~/…" for ones inside the home folder.
    pub excluded: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            lang: None,
            theme: Theme::System,
            style: UiStyle::Standard,
            stale_days: 180,
            junk_days: 30,
            dupes_min_mb: 1,
            scan_on_start: true,
            menu_bar: true,
            check_updates: true,
            notifications: true,
            auto_clean_days: 0,
            auto_clean_last: 0,
            file_access: false,
            excluded: Vec::new(),
        }
    }
}

pub fn dir() -> PathBuf {
    crate::home().join("Library/Application Support/MacPilot")
}

fn file() -> PathBuf {
    dir().join("settings.conf")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(file()) else { return s };
        // Settings from before the question existed: the user has been using Disk and Cleanup already.
        s.file_access = !text.lines().any(|l| l.trim_start().starts_with("file_access"));
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "lang" => s.lang = Lang::from_code(v),
                "theme" => {
                    s.theme = match v {
                        "light" => Theme::Light,
                        "dark" => Theme::Dark,
                        _ => Theme::System,
                    }
                }
                "style" => s.style = if v == "classic" { UiStyle::Classic } else { UiStyle::Standard },
                "stale_days" => s.stale_days = v.parse().unwrap_or(s.stale_days),
                "junk_days" => s.junk_days = v.parse().unwrap_or(s.junk_days),
                "dupes_min_mb" => s.dupes_min_mb = v.parse().unwrap_or(s.dupes_min_mb),
                "scan_on_start" => s.scan_on_start = v != "false",
                "menu_bar" => s.menu_bar = v != "false",
                "check_updates" => s.check_updates = v != "false",
                "notifications" => s.notifications = v != "false",
                "auto_clean_days" => s.auto_clean_days = v.parse().unwrap_or(0),
                "auto_clean_last" => s.auto_clean_last = v.parse().unwrap_or(0),
                "file_access" => s.file_access = v == "true",
                // One folder per line: paths may contain commas.
                "excluded" => s.excluded.push(v.to_string()),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(dir())?;
        let theme = match self.theme {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        };
        let text = format!(
            "# MacPilot settings\nlang = {}\ntheme = {theme}\nstyle = {}\nstale_days = {}\njunk_days = {}\ndupes_min_mb = {}\nscan_on_start = {}\nmenu_bar = {}\ncheck_updates = {}\nnotifications = {}\nauto_clean_days = {}\nauto_clean_last = {}\nfile_access = {}\n{}",
            self.lang.map(|l| l.code()).unwrap_or("system"),
            if self.style == UiStyle::Classic { "classic" } else { "standard" },
            self.stale_days,
            self.junk_days,
            self.dupes_min_mb,
            self.scan_on_start,
            self.menu_bar,
            self.check_updates,
            self.notifications,
            self.auto_clean_days,
            self.auto_clean_last,
            self.file_access,
            self.excluded.iter().map(|e| format!("excluded = {e}\n")).collect::<String>()
        );
        std::fs::write(file(), text)
    }

    /// Excluded folders as full paths.
    pub fn excluded_paths(&self) -> Vec<PathBuf> {
        self.excluded.iter().map(|e| expand(e)).collect()
    }

    pub fn is_excluded(&self, p: &std::path::Path) -> bool {
        self.excluded.iter().any(|e| expand(e) == p)
    }

    /// Exclude a folder (or include it again).
    pub fn set_excluded(&mut self, p: &std::path::Path, excluded: bool) {
        let short = shorten(p);
        self.excluded.retain(|e| expand(e) != p);
        if excluded {
            self.excluded.push(short);
        }
    }

    /// Apply the language to the translation layer.
    pub fn apply_lang(&self) {
        crate::i18n::set_lang(self.lang.unwrap_or_else(Lang::system));
    }
}

/// "~/Downloads" → "/Users/me/Downloads".
pub fn expand(s: &str) -> PathBuf {
    match s.strip_prefix("~/") {
        Some(rest) => crate::home().join(rest),
        None if s == "~" => crate::home(),
        None => PathBuf::from(s),
    }
}

/// "/Users/me/Downloads" → "~/Downloads" (kept readable in the settings file).
pub fn shorten(p: &std::path::Path) -> String {
    match p.strip_prefix(crate::home()) {
        Ok(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => p.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_folders_round_trip() {
        let mut s = Settings::default();
        let dl = crate::home().join("Downloads");
        s.set_excluded(&dl, true);
        s.set_excluded(std::path::Path::new("/Volumes/Backup, old"), true);
        assert_eq!(s.excluded, vec!["~/Downloads".to_string(), "/Volumes/Backup, old".to_string()]);
        assert!(s.is_excluded(&dl));
        s.set_excluded(&dl, false);
        assert_eq!(s.excluded_paths(), vec![PathBuf::from("/Volumes/Backup, old")]);
    }
}
