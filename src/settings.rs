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

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `None` = follow the macOS language.
    pub lang: Option<Lang>,
    pub theme: Theme,
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            lang: None,
            theme: Theme::System,
            stale_days: 180,
            junk_days: 30,
            dupes_min_mb: 1,
            scan_on_start: true,
            menu_bar: true,
            check_updates: true,
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
                "stale_days" => s.stale_days = v.parse().unwrap_or(s.stale_days),
                "junk_days" => s.junk_days = v.parse().unwrap_or(s.junk_days),
                "dupes_min_mb" => s.dupes_min_mb = v.parse().unwrap_or(s.dupes_min_mb),
                "scan_on_start" => s.scan_on_start = v != "false",
                "menu_bar" => s.menu_bar = v != "false",
                "check_updates" => s.check_updates = v != "false",
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
            "# MacPilot settings\nlang = {}\ntheme = {theme}\nstale_days = {}\njunk_days = {}\ndupes_min_mb = {}\nscan_on_start = {}\nmenu_bar = {}\ncheck_updates = {}\n",
            self.lang.map(|l| l.code()).unwrap_or("system"),
            self.stale_days,
            self.junk_days,
            self.dupes_min_mb,
            self.scan_on_start,
            self.menu_bar,
            self.check_updates
        );
        std::fs::write(file(), text)
    }

    /// Apply the language to the translation layer.
    pub fn apply_lang(&self) {
        crate::i18n::set_lang(self.lang.unwrap_or_else(Lang::system));
    }
}
