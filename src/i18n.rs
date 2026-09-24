//! Translations. English text is the key; other languages come from `strings.rs`.
//! Missing entries fall back to English, and a test checks that every key is translated.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

mod strings;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Lang {
    En,
    Ru,
    Fr,
    Es,
    De,
}

impl Lang {
    pub const ALL: [Lang; 5] = [Lang::En, Lang::Fr, Lang::Es, Lang::De, Lang::Ru];

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ru => "ru",
            Lang::Fr => "fr",
            Lang::Es => "es",
            Lang::De => "de",
        }
    }

    /// The language's own name, for the language picker.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
            Lang::Fr => "Français",
            Lang::Es => "Español",
            Lang::De => "Deutsch",
        }
    }

    pub fn from_code(s: &str) -> Option<Lang> {
        let s = s.trim().to_lowercase();
        Lang::ALL.into_iter().find(|l| s.starts_with(l.code()))
    }

    /// Language of the macOS user interface (falls back to English).
    pub fn system() -> Lang {
        let out = crate::run_output("defaults", &["read", "-g", "AppleLanguages"]);
        let first = out.split(['"', '(', ')', ',', '\n', ' ']).find(|s| s.len() >= 2);
        first.and_then(Lang::from_code).or_else(|| std::env::var("LANG").ok().and_then(|l| Lang::from_code(&l))).unwrap_or(Lang::En)
    }

    fn index(self) -> usize {
        match self {
            Lang::En => 0,
            Lang::Ru => 1,
            Lang::Fr => 2,
            Lang::Es => 3,
            Lang::De => 4,
        }
    }

    fn from_index(i: u8) -> Lang {
        match i {
            1 => Lang::Ru,
            2 => Lang::Fr,
            3 => Lang::Es,
            4 => Lang::De,
            _ => Lang::En,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    CURRENT.store(l.index() as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    Lang::from_index(CURRENT.load(Ordering::Relaxed))
}

fn table() -> &'static HashMap<&'static str, [&'static str; 4]> {
    static T: OnceLock<HashMap<&'static str, [&'static str; 4]>> = OnceLock::new();
    T.get_or_init(|| strings::TABLE.iter().map(|(en, ru, fr, es, de)| (*en, [*ru, *fr, *es, *de])).collect())
}

/// Translate an English UI string into the current language.
pub fn tr(en: &'static str) -> &'static str {
    let l = lang();
    if l == Lang::En {
        return en;
    }
    match table().get(en) {
        Some(row) => row[l.index() - 1],
        None => en,
    }
}

/// Translate and fill `{0}`, `{1}`… placeholders.
pub fn trf(en: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let mut s = tr(en).to_string();
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), &a.to_string());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every literal passed to tr/trf in the sources must have a translation row.
    #[test]
    fn all_keys_translated() {
        let mut missing = Vec::new();
        let dirs = ["src", "src/gui"];
        for d in dirs {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.extension().is_none_or(|x| x != "rs") || p.ends_with("strings.rs") || p.ends_with("i18n.rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&p).unwrap();
                for key in extract_keys(&src) {
                    if !table().contains_key(key.as_str()) {
                        missing.push(format!("{}: {key}", p.display()));
                    }
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "untranslated keys:\n{}", missing.join("\n"));
    }

    #[test]
    fn no_duplicate_rows() {
        let mut seen = std::collections::HashSet::new();
        for (en, ..) in strings::TABLE {
            assert!(seen.insert(*en), "duplicate row: {en}");
        }
    }

    #[test]
    fn placeholders_match() {
        for (en, ru, fr, es, de) in strings::TABLE {
            let count = |s: &str| (0..6).filter(|i| s.contains(&format!("{{{i}}}"))).count();
            let n = count(en);
            for t in [ru, fr, es, de] {
                assert_eq!(count(t), n, "placeholder mismatch: {en} -> {t}");
            }
        }
    }

    /// Pull string literals passed to `tr(` or `trf(` (rustfmt may put a line break after the parenthesis).
    fn extract_keys(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for pat in ["tr(", "trf("] {
            let mut rest = src;
            while let Some(i) = rest.find(pat) {
                // Skip identifiers that merely end in "tr", like `attr(`.
                let before = rest[..i].chars().last();
                rest = &rest[i + pat.len()..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let trimmed = rest.trim_start();
                let Some(after_quote) = trimmed.strip_prefix('"') else { continue };
                rest = after_quote;
                let mut key = String::new();
                let mut chars = rest.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => match chars.next() {
                            Some('n') => key.push('\n'),
                            Some('"') => key.push('"'),
                            Some('\\') => key.push('\\'),
                            Some('\n') => {
                                // Line continuation: skip leading whitespace on the next line.
                                let s: String = chars.clone().collect();
                                let trimmed = s.trim_start();
                                let skipped = s.len() - trimmed.len();
                                for _ in 0..s[..skipped].chars().count() {
                                    chars.next();
                                }
                            }
                            Some(o) => key.push(o),
                            None => {}
                        },
                        '"' => break,
                        c => key.push(c),
                    }
                }
                out.push(key);
            }
        }
        out
    }
}
