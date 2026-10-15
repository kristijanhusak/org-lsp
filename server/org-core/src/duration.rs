//! `org-duration`: durations as Org writes them (`org-duration-from-minutes`)
//! and reads them (`org-duration-to-minutes`), by `org-duration-format`
//! and `org-duration-units`.
//!
//! As Emacs's variables, the format and the units are the server's for
//! all it writes and reads; they are kept for the thread it runs on, set
//! once from its settings ([`set`]).

use std::cell::RefCell;

use regex::Regex;

/// A unit and the minutes it stands for (`org-duration-units`). Lisp tells
/// an integer modifier from a float one, and divides by them differently.
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub name: String,
    pub minutes: f64,
    /// Written as a float (`(* 60 24 365.25)`).
    pub float: bool,
}

impl Unit {
    fn new(name: &str, minutes: f64, float: bool) -> Unit {
        Unit { name: name.to_string(), minutes, float }
    }
}

/// `org-duration-canonical-units`
fn canonical() -> Vec<Unit> {
    vec![Unit::new("min", 1.0, false), Unit::new("h", 60.0, false), Unit::new("d", 1440.0, false)]
}

/// The default of `org-duration-units`.
pub fn default_units() -> Vec<Unit> {
    let mut units = canonical();
    units.extend([Unit::new("w", 10_080.0, false), Unit::new("m", 43_200.0, false), Unit::new("y", 525_960.0, true)]);
    units
}

/// How a duration is written (`org-duration-format`).
#[derive(Debug, Clone, PartialEq)]
pub enum Format {
    /// `h:mm`
    HMm,
    /// `h:mm:ss`
    HMmSs,
    /// A list of units, each with whether it is written when zero, and
    /// perhaps a `special` entry, and `compact`.
    Units { units: Vec<(String, bool)>, special: Option<Special>, compact: bool },
}

/// The `special` entry of a list format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    /// The hours and minutes as `H:MM`, the larger units before.
    HMm,
    /// As `H:MM:SS`.
    HMmSs,
    /// One unit, with that many decimals.
    Precision(usize),
}

impl Default for Format {
    /// `(("d" . nil) (special . h:mm))`
    fn default() -> Self {
        Format::Units { units: vec![("d".to_string(), false)], special: Some(Special::HMm), compact: false }
    }
}

/// The format and the units durations are written and read with.
#[derive(Debug, Clone, PartialEq)]
pub struct Durations {
    pub format: Format,
    pub units: Vec<Unit>,
}

impl Default for Durations {
    fn default() -> Self {
        Durations { format: Format::default(), units: default_units() }
    }
}

impl Durations {
    /// Whether every unit the format names is one of the units, as
    /// `org-duration--modifier` requires.
    pub fn is_valid(&self) -> bool {
        match &self.format {
            Format::Units { units, .. } => {
                units.iter().all(|(name, _)| self.units.iter().any(|unit| unit.name == *name))
            }
            _ => true,
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Durations> = RefCell::new(Durations::default());
}

/// Write and read durations with `durations` from now on, on this thread.
pub fn set(durations: Durations) {
    CURRENT.with(|current| *current.borrow_mut() = durations);
}

/// The format and units in use.
pub fn current() -> Durations {
    CURRENT.with(|current| current.borrow().clone())
}

/// `org-duration-from-minutes` with the format and units in use.
pub fn from_minutes(minutes: f64) -> String {
    let durations = current();
    from_minutes_with(minutes, &durations.format, &durations.units, false)
}

/// `org-duration-to-minutes` with the units in use: none for what is no
/// duration.
pub fn to_minutes(text: &str) -> Option<f64> {
    to_minutes_with(text, &current().units, false)
}

/// The unit called `name`: among the canonical units with `canonical`.
fn modifier(name: &str, units: &[Unit], canonical_units: bool) -> Unit {
    let list = if canonical_units { canonical() } else { units.to_vec() };
    // An unknown unit is refused when the settings are read.
    list.into_iter().find(|unit| unit.name == name).unwrap_or_else(|| Unit::new(name, 1.0, false))
}

/// `org-duration-from-minutes`: `minutes` written as `format` says.
pub fn from_minutes_with(minutes: f64, format: &Format, units: &[Unit], canonical_units: bool) -> String {
    if minutes < 0.0 {
        return format!("-{}", from_minutes_with(-minutes, format, units, canonical_units));
    }
    match format {
        Format::HMm => format!("{}:{:02}", (minutes / 60.0).trunc() as i64, (minutes % 60.0).trunc() as i64),
        Format::HMmSs => {
            let whole = minutes.floor();
            let seconds = (60.0 * minutes) % 60.0;
            format!("{}:{:02}", from_minutes_with(whole, &Format::HMm, units, canonical_units), seconds.trunc() as i64)
        }
        // Units and H:MM, or H:MM:SS, for what is left past them.
        Format::Units { units: list, special: Some(special @ (Special::HMm | Special::HMmSs)), compact } => {
            let mode = if *special == Special::HMm { Format::HMm } else { Format::HMmSs };
            let larger: Vec<(String, bool)> = list
                .iter()
                .filter(|(name, _)| modifier(name, units, canonical_units).minutes > 60.0)
                .cloned()
                .collect();
            let smallest = larger
                .iter()
                .map(|(name, _)| modifier(name, units, canonical_units))
                .min_by(|a, b| a.minutes.total_cmp(&b.minutes));
            match smallest {
                Some(unit) if minutes >= unit.minutes => {
                    // `(* min-modifier (/ (floor minutes) min-modifier))`: a
                    // division of integers drops the rest.
                    let quotient = minutes.floor() / unit.minutes;
                    let quotient = if unit.float { quotient } else { quotient.floor() };
                    let units_part = unit.minutes * quotient;
                    let truncated = Format::Units { units: larger, special: None, compact: false };
                    format!(
                        "{}{}{}",
                        from_minutes_with(units_part, &truncated, units, canonical_units),
                        if *compact { "" } else { " " },
                        from_minutes_with(minutes - units_part, &mode, units, false)
                    )
                }
                _ => from_minutes_with(minutes, &mode, units, canonical_units),
            }
        }
        Format::Units { units: list, special, compact } => {
            let mut selected: Vec<(Unit, bool)> =
                list.iter().map(|(name, required)| (modifier(name, units, canonical_units), *required)).collect();
            // `sort` is stable: equal modifiers keep their order.
            selected.sort_by(|a, b| b.0.minutes.total_cmp(&a.0.minutes));
            let separator = if *compact { "" } else { " " };
            let Some(last) = selected.last().map(|(unit, _)| unit.clone()) else {
                return String::new();
            };
            if let Some(Special::Precision(digits)) = special {
                // The first unit required, or not larger than the duration.
                let unit = selected
                    .iter()
                    .find(|(unit, required)| *required || unit.minutes <= minutes)
                    .map_or(last, |(unit, _)| unit.clone());
                return format!("{:.*}{}", digits, minutes / unit.minutes, unit.name);
            }
            let mut left = minutes;
            let mut out = String::new();
            for (unit, required) in &selected {
                if unit.minutes <= left {
                    let value = (left / unit.minutes).floor();
                    left -= value * unit.minutes;
                    out.push_str(&format!("{separator}{}{}", value as i64, unit.name));
                } else if *required {
                    out.push_str(&format!("{separator}0{}", unit.name));
                }
            }
            let out = out.trim_matches([' ', '\t', '\n', '\r']).to_string();
            if out.is_empty() {
                format!("0{}", last.name)
            } else {
                out
            }
        }
    }
}

/// `org-duration--h:mm-re`
fn h_mm() -> &'static Regex {
    static RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"^[ \t]*[0-9]+(?::[0-9]{2}){1,2}[ \t]*$").expect("a valid regexp"));
    &RE
}

/// `org-duration--unit-re`: a number and a unit, canonical or not, the
/// longest names tried first as `regexp-opt` does.
fn unit_re(units: &[Unit]) -> String {
    let mut names: Vec<String> = canonical().into_iter().chain(units.iter().cloned()).map(|unit| unit.name).collect();
    names.sort_by_key(|name| std::cmp::Reverse(name.len()));
    names.dedup();
    let alternatives: Vec<String> = names.iter().map(|name| regex::escape(name)).collect();
    format!(r"([0-9]+(?:\.[0-9]*)?)[ \t]*({})", alternatives.join("|"))
}

/// `org-duration-p`
pub fn is_duration(text: &str, units: &[Unit]) -> bool {
    let unit = unit_re(units);
    let full = Regex::new(&format!(r"^(?:[ \t]*{unit})+[ \t]*$")).expect("a valid regexp");
    let mixed = Regex::new(&format!(r"^(?:[ \t]*{unit})+[ \t]*[0-9]+(?::[0-9][0-9]){{1,2}}[ \t]*$")).expect("valid");
    full.is_match(text) || mixed.is_match(text) || h_mm().is_match(text)
}

/// `org-duration-to-minutes`: the empty text is 0, a bare number minutes;
/// none for what is no duration.
pub fn to_minutes_with(text: &str, units: &[Unit], canonical_units: bool) -> Option<f64> {
    if text.is_empty() {
        return Some(0.0);
    }
    if h_mm().is_match(text) {
        let numbers: Vec<f64> = text.trim().split(':').map(|part| part.parse().unwrap_or(0.0)).collect();
        return Some(numbers.get(2).copied().unwrap_or(0.0) / 60.0 + numbers[1] + 60.0 * numbers[0]);
    }
    let unit = unit_re(units);
    let one = Regex::new(&unit).expect("a valid regexp");
    let full = Regex::new(&format!(r"^(?:[ \t]*{unit})+[ \t]*$")).expect("a valid regexp");
    if full.is_match(text) {
        let mut minutes = 0.0;
        for found in one.captures_iter(text) {
            let value: f64 = found[1].parse().unwrap_or(0.0);
            let unit = modifier(&found[2], units, canonical_units);
            if !canonical_units && !units.iter().any(|known| known.name == unit.name) {
                // `Unknown unit` in Emacs.
                return None;
            }
            minutes += value * unit.minutes;
        }
        return Some(minutes);
    }
    let mixed = Regex::new(&format!(r"^((?:[ \t]*{unit})+)[ \t]*([0-9]+(?::[0-9][0-9]){{1,2}})[ \t]*$"))
        .expect("a valid regexp");
    if let Some(found) = mixed.captures(text) {
        let (units_part, hms) = (found.get(1)?.as_str(), found.get(found.len() - 1)?.as_str());
        return Some(to_minutes_with(units_part, units, false)? + to_minutes_with(hms, units, false)?);
    }
    let bare = Regex::new(r"^[0-9]+(\.[0-9]*)?$").expect("a valid regexp");
    bare.is_match(text).then(|| text.parse().unwrap_or(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_with_the_default_format() {
        let units = default_units();
        let write = |minutes: f64| from_minutes_with(minutes, &Format::default(), &units, false);
        assert_eq!(write(0.0), "0:00");
        assert_eq!(write(75.0), "1:15");
        assert_eq!(write(1565.0), "1d 2:05");
        assert_eq!(write(-90.0), "-1:30");
    }

    #[test]
    fn reads_units_and_hours() {
        let units = default_units();
        let read = |text: &str| to_minutes_with(text, &units, false);
        assert_eq!(read("1:30"), Some(90.0));
        assert_eq!(read("1h 30min"), Some(90.0));
        assert_eq!(read("1d 2:00"), Some(1560.0));
        assert_eq!(read("soon"), None);
    }
}
