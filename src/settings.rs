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

/// Look of the app: the standard one, or one after a classic computer or console.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UiStyle {
    /// The standard look.
    Standard,
    /// Black and white like the first Macintosh: 1-pixel lines, square corners, pixel titles.
    Classic,
    /// Gray 3D bevels, navy title bars.
    Win98,
    /// Luna: blue title bars, green buttons, beige panels.
    WinXp,
    /// The gray, black and red of the NES.
    Nes,
    /// Four shades of green.
    GameBoy,
    /// PlayStation gray with the colors of its four buttons.
    Ps1,
    /// The dark blue glow of the PlayStation 2 browser.
    Ps2,
}

impl UiStyle {
    pub const ALL: [UiStyle; 8] =
        [UiStyle::Standard, UiStyle::Classic, UiStyle::Win98, UiStyle::WinXp, UiStyle::Nes, UiStyle::GameBoy, UiStyle::Ps1, UiStyle::Ps2];

    pub fn code(self) -> &'static str {
        match self {
            UiStyle::Standard => "standard",
            UiStyle::Classic => "classic",
            UiStyle::Win98 => "win98",
            UiStyle::WinXp => "winxp",
            UiStyle::Nes => "nes",
            UiStyle::GameBoy => "gameboy",
            UiStyle::Ps1 => "ps1",
            UiStyle::Ps2 => "ps2",
        }
    }

    pub fn from_code(s: &str) -> Option<UiStyle> {
        UiStyle::ALL.into_iter().find(|p| p.code() == s)
    }

    pub fn name(self) -> &'static str {
        match self {
            UiStyle::Standard => crate::tr("Standard"),
            UiStyle::Classic => crate::tr("Classic Macintosh"),
            UiStyle::Win98 => "Windows 98",
            UiStyle::WinXp => "Windows XP",
            UiStyle::Nes => "Nintendo",
            UiStyle::GameBoy => "Game Boy",
            UiStyle::Ps1 => "PlayStation",
            UiStyle::Ps2 => "PlayStation 2",
        }
    }

    /// Styles that are light or dark by nature: `Some(true)` is always dark. The standard look
    /// and the Macintosh follow the Appearance setting.
    pub fn fixed_dark(self) -> Option<bool> {
        match self {
            UiStyle::Standard | UiStyle::Classic => None,
            UiStyle::Ps2 => Some(true),
            _ => Some(false),
        }
    }
}

/// Colors of the standard style: an accent and tinted backgrounds, each for light and dark.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Palette {
    /// Blue on neutral gray — the look MacPilot always had.
    Default,
    Graphite,
    Ocean,
    Forest,
    Sunset,
    Rose,
    Nord,
    Dracula,
    Solarized,
    /// True black in the dark theme (for OLED screens).
    Midnight,
}

impl Palette {
    pub const ALL: [Palette; 10] = [
        Palette::Default,
        Palette::Graphite,
        Palette::Ocean,
        Palette::Forest,
        Palette::Sunset,
        Palette::Rose,
        Palette::Nord,
        Palette::Dracula,
        Palette::Solarized,
        Palette::Midnight,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Palette::Default => "default",
            Palette::Graphite => "graphite",
            Palette::Ocean => "ocean",
            Palette::Forest => "forest",
            Palette::Sunset => "sunset",
            Palette::Rose => "rose",
            Palette::Nord => "nord",
            Palette::Dracula => "dracula",
            Palette::Solarized => "solarized",
            Palette::Midnight => "midnight",
        }
    }

    pub fn from_code(s: &str) -> Option<Palette> {
        Palette::ALL.into_iter().find(|p| p.code() == s)
    }

    pub fn name(self) -> &'static str {
        use crate::tr;
        match self {
            Palette::Default => "MacPilot",
            Palette::Graphite => tr("Graphite"),
            Palette::Ocean => tr("Ocean"),
            Palette::Forest => tr("Forest"),
            Palette::Sunset => tr("Sunset"),
            Palette::Rose => tr("Rose"),
            Palette::Nord => "Nord",
            Palette::Dracula => "Dracula",
            Palette::Solarized => "Solarized",
            Palette::Midnight => tr("Midnight"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `None` = follow the macOS language.
    pub lang: Option<Lang>,
    pub theme: Theme,
    pub style: UiStyle,
    pub palette: Palette,
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
            palette: Palette::Default,
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
                "style" => s.style = UiStyle::from_code(v).unwrap_or(UiStyle::Standard),
                "palette" => s.palette = Palette::from_code(v).unwrap_or(Palette::Default),
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
            "# MacPilot settings\nlang = {}\ntheme = {theme}\nstyle = {}\npalette = {}\nstale_days = {}\njunk_days = {}\ndupes_min_mb = {}\nscan_on_start = {}\nmenu_bar = {}\ncheck_updates = {}\nnotifications = {}\nauto_clean_days = {}\nauto_clean_last = {}\nfile_access = {}\n{}",
            self.lang.map(|l| l.code()).unwrap_or("system"),
            self.style.code(),
            self.palette.code(),
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
    fn palette_codes_round_trip() {
        for p in Palette::ALL {
            assert_eq!(Palette::from_code(p.code()), Some(p));
        }
        for st in UiStyle::ALL {
            assert_eq!(UiStyle::from_code(st.code()), Some(st));
        }
        assert_eq!(Palette::from_code("no such theme"), None);
    }

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
