//! Locale-aware formatting of sizes, counts, dates and ages.

use crate::i18n::{Lang, lang};

fn decimal_comma() -> bool {
    lang() != Lang::En
}

fn fmt_float(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if decimal_comma() { s.replace('.', ",") } else { s }
}

/// Size in bytes, SI units like Finder (1 KB = 1000 B).
pub fn bytes(b: u64) -> String {
    let units: [&str; 6] = match lang() {
        Lang::Ru => ["Б", "КБ", "МБ", "ГБ", "ТБ", "ПБ"],
        Lang::Fr => ["o", "Ko", "Mo", "Go", "To", "Po"],
        _ => ["B", "KB", "MB", "GB", "TB", "PB"],
    };
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1000.0 && i < units.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    let num = if i == 0 {
        b.to_string()
    } else if v < 10.0 {
        fmt_float(v, 2)
    } else if v < 100.0 {
        fmt_float(v, 1)
    } else {
        fmt_float(v, 0)
    };
    format!("{num} {}", units[i])
}

/// Integer with thousands separators.
pub fn count(n: u64) -> String {
    let sep = match lang() {
        Lang::En => ',',
        Lang::Es | Lang::De => '.',
        Lang::Ru | Lang::Fr => '\u{202F}',
    };
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Short duration: "3d 4h", "5h 12m".
pub fn duration(secs: u64) -> String {
    let (d, h, m, s) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let (ud, uh, um, us) = match lang() {
        Lang::Ru => ("д", "ч", "м", "с"),
        Lang::Fr => ("j", "h", "min", "s"),
        Lang::De => ("T", "Std", "Min", "s"),
        _ => ("d", "h", "m", "s"),
    };
    if d > 0 {
        format!("{d}{ud} {h}{uh}")
    } else if h > 0 {
        format!("{h}{uh} {m}{um}")
    } else if m > 0 {
        format!("{m}{um} {s}{us}")
    } else {
        format!("{s}{us}")
    }
}

/// Path with the home folder shortened to `~`.
pub fn path(p: &std::path::Path) -> String {
    let home = crate::home();
    if let Ok(rest) = p.strip_prefix(&home) {
        if rest.as_os_str().is_empty() {
            return "~".into();
        }
        return format!("~/{}", rest.display());
    }
    p.display().to_string()
}

/// Russian plural: plural(5, "день", "дня", "дней") → "дней".
pub fn plural_ru<'a>(n: i64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let n = n.abs();
    let (d10, d100) = (n % 10, n % 100);
    if d10 == 1 && d100 != 11 {
        one
    } else if (2..=4).contains(&d10) && !(12..=14).contains(&d100) {
        few
    } else {
        many
    }
}

#[derive(Clone, Copy)]
enum Unit {
    Hour,
    Day,
    Month,
    Year,
}

/// Unit word for `n`. `dative` selects German dative plural ("vor 3 Tagen").
fn unit(n: i64, u: Unit, dative: bool) -> &'static str {
    let one = n == 1;
    match lang() {
        Lang::En => match u {
            Unit::Hour => {
                if one {
                    "hour"
                } else {
                    "hours"
                }
            }
            Unit::Day => {
                if one {
                    "day"
                } else {
                    "days"
                }
            }
            Unit::Month => {
                if one {
                    "month"
                } else {
                    "months"
                }
            }
            Unit::Year => {
                if one {
                    "year"
                } else {
                    "years"
                }
            }
        },
        Lang::Ru => match u {
            Unit::Hour => plural_ru(n, "час", "часа", "часов"),
            Unit::Day => plural_ru(n, "день", "дня", "дней"),
            Unit::Month => plural_ru(n, "месяц", "месяца", "месяцев"),
            Unit::Year => plural_ru(n, "год", "года", "лет"),
        },
        Lang::Fr => match u {
            Unit::Hour => {
                if one {
                    "heure"
                } else {
                    "heures"
                }
            }
            Unit::Day => {
                if one {
                    "jour"
                } else {
                    "jours"
                }
            }
            Unit::Month => "mois",
            Unit::Year => {
                if one {
                    "an"
                } else {
                    "ans"
                }
            }
        },
        Lang::Es => match u {
            Unit::Hour => {
                if one {
                    "hora"
                } else {
                    "horas"
                }
            }
            Unit::Day => {
                if one {
                    "día"
                } else {
                    "días"
                }
            }
            Unit::Month => {
                if one {
                    "mes"
                } else {
                    "meses"
                }
            }
            Unit::Year => {
                if one {
                    "año"
                } else {
                    "años"
                }
            }
        },
        Lang::De => match (u, one, dative) {
            (Unit::Hour, true, _) => "Stunde",
            (Unit::Hour, false, _) => "Stunden",
            (Unit::Day, true, _) => "Tag",
            (Unit::Day, false, true) => "Tagen",
            (Unit::Day, false, false) => "Tage",
            (Unit::Month, true, _) => "Monat",
            (Unit::Month, false, true) => "Monaten",
            (Unit::Month, false, false) => "Monate",
            (Unit::Year, true, _) => "Jahr",
            (Unit::Year, false, true) => "Jahren",
            (Unit::Year, false, false) => "Jahre",
        },
    }
}

fn age_impl(ts: i64, dative: bool) -> String {
    let secs = (crate::disk::now_unix() - ts).max(0);
    let days = secs / 86_400;
    if days == 0 {
        let h = secs / 3600;
        if h == 0 {
            return String::new();
        }
        return format!("{h} {}", unit(h, Unit::Hour, dative));
    }
    if days < 31 {
        return format!("{days} {}", unit(days, Unit::Day, dative));
    }
    let months = days * 12 / 365;
    if months < 12 {
        return format!("{months} {}", unit(months, Unit::Month, dative));
    }
    let years = months / 12;
    let rest = months % 12;
    let y = format!("{years} {}", unit(years, Unit::Year, dative));
    if rest == 0 || years >= 3 { y } else { format!("{y} {rest} {}", unit(rest, Unit::Month, dative)) }
}

/// How long ago, as a bare amount: "3 days", "1 year 4 months".
pub fn age(ts: i64) -> String {
    let a = age_impl(ts, false);
    if a.is_empty() { crate::tr("less than an hour").into() } else { a }
}

/// Amount for use inside "not used for {0}" sentences (German needs the dative).
pub fn age_in(ts: i64) -> String {
    let a = age_impl(ts, true);
    if a.is_empty() { crate::tr("less than an hour").into() } else { a }
}

/// "2 years ago" / "just now".
pub fn ago(ts: i64) -> String {
    let a = age_impl(ts, true);
    if a.is_empty() { crate::tr("just now").into() } else { crate::trf("{0} ago", &[&a]) }
}

/// Local date: "12 Mar 2024".
pub fn date(ts: i64) -> String {
    let t = ts as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    let months: [&str; 12] = match lang() {
        Lang::Ru => ["янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"],
        Lang::Fr => ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."],
        Lang::Es => ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"],
        Lang::De => ["Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez."],
        Lang::En => ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
    };
    let m = months[tm.tm_mon.clamp(0, 11) as usize];
    let (d, y) = (tm.tm_mday, tm.tm_year + 1900);
    match lang() {
        Lang::De => format!("{d}. {m} {y}"),
        _ => format!("{d} {m} {y}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::set_lang;

    #[test]
    fn russian_plurals() {
        assert_eq!(plural_ru(1, "a", "b", "c"), "a");
        assert_eq!(plural_ru(3, "a", "b", "c"), "b");
        assert_eq!(plural_ru(11, "a", "b", "c"), "c");
        assert_eq!(plural_ru(22, "a", "b", "c"), "b");
    }

    #[test]
    fn sizes_are_localized() {
        set_lang(Lang::En);
        assert_eq!(bytes(2_870_000_000), "2.87 GB");
        set_lang(Lang::Fr);
        assert_eq!(bytes(2_870_000_000), "2,87 Go");
        set_lang(Lang::En);
    }
}
