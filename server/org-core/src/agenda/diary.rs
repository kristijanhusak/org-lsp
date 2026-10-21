//! Diary sexps: `%%(diary-anniversary 5 18 1990) Birthday` lines and
//! `<%%(diary-float t 4 2)>` timestamps. Emacs evaluates them as Lisp for
//! each date (`org-diary-sexp-entry`); here the date functions of
//! `diary-lib` (`diary-date`, `diary-block`, `diary-float`,
//! `diary-anniversary`, `diary-cyclic`, with `calendar-date-style`
//! `american`) and Org's own (`org-date`, `org-block`, `org-anniversary`,
//! `org-cyclic`, `org-class`) are, with `and`, `or`, `not`, comparisons,
//! `memq` and the calendar's accessors of `date`. Anything else goes to
//! Emacs (see [`Outside`]), or does not apply.

use crate::date::{days_in_month, weekday, Date, Day};

/// A Lisp value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Nil,
    T,
    Int(i64),
    Str(String),
    Symbol(String),
    List(Vec<Value>),
    /// What a diary function returns when it applies: `(MARK . ENTRY)`.
    Entry(String),
}

impl Value {
    fn truthy(&self) -> bool {
        !matches!(self, Value::Nil) && *self != Value::List(Vec::new())
    }
}

/// Read one Lisp form from `text`.
fn read(text: &str) -> Option<Value> {
    let mut chars = text.chars().peekable();
    read_form(&mut chars, 0)
}

/// One form, `depth` lists deep: deeper than 200 is not read, rather
/// than read until the stack runs out.
fn read_form(chars: &mut std::iter::Peekable<std::str::Chars>, depth: usize) -> Option<Value> {
    if depth == 200 {
        return None;
    }
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
    match *chars.peek()? {
        '(' => {
            chars.next();
            let mut items = Vec::new();
            loop {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                match chars.peek()? {
                    ')' => {
                        chars.next();
                        return Some(Value::List(items));
                    }
                    _ => items.push(read_form(chars, depth + 1)?),
                }
            }
        }
        '\'' => {
            chars.next();
            let quoted = read_form(chars, depth + 1)?;
            Some(Value::List(vec![Value::Symbol("quote".to_string()), quoted]))
        }
        '"' => {
            chars.next();
            let mut text = String::new();
            loop {
                match chars.next()? {
                    '"' => return Some(Value::Str(text)),
                    '\\' => text.push(chars.next()?),
                    c => text.push(c),
                }
            }
        }
        _ => {
            let mut atom = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == '(' || c == ')' {
                    break;
                }
                atom.push(c);
                chars.next();
            }
            Some(match atom.as_str() {
                "nil" => Value::Nil,
                "t" => Value::T,
                _ => match atom.parse::<i64>() {
                    Ok(number) => Value::Int(number),
                    Err(_) => Value::Symbol(atom),
                },
            })
        }
    }
}

/// The date the sexp is evaluated for, and the entry text it may give.
struct Scope<'a> {
    date: Date,
    day: Day,
    entry: &'a str,
}

/// `calendar-absolute-from-gregorian`, as days the way this crate counts.
fn absolute(month: i64, day: i64, year: i64) -> Day {
    Date::new(year, month, 1).to_day() + day - 1
}

/// `calendar-dayname-on-or-before`
fn dayname_on_or_before(dayname: i64, date: Day) -> Day {
    date - (weekday(date) as i64 - dayname).rem_euclid(7)
}

/// `calendar-nth-named-absday`
fn nth_named_absday(n: i64, dayname: i64, month: i64, year: i64, day: Option<i64>) -> Day {
    if n > 0 {
        7 * (n - 1) + dayname_on_or_before(dayname, 6 + absolute(month, day.unwrap_or(1), year))
    } else {
        let day = day.unwrap_or_else(|| days_in_month(year, month));
        7 * (n + 1) + dayname_on_or_before(dayname, absolute(month, day, year))
    }
}

/// `diary-ordinal-suffix`
fn ordinal_suffix(n: i64) -> &'static str {
    if [11, 12, 13].contains(&(n % 100)) || n % 10 > 3 {
        "th"
    } else {
        ["th", "st", "nd", "rd"][(n % 10).max(0) as usize]
    }
}

/// `(format entry n (diary-ordinal-suffix n))`, for the `%d` and `%s` of
/// an anniversary or a cycle.
fn format_count(entry: &str, n: i64) -> String {
    let mut out = String::new();
    let mut arguments = [n.to_string(), ordinal_suffix(n).to_string()].into_iter();
    let mut chars = entry.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('d') | Some('s') => out.push_str(&arguments.next().unwrap_or_default()),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// Whether `value` (an integer, `t` or a list of integers) holds `wanted`.
fn matches(value: &Value, wanted: i64) -> bool {
    match value {
        Value::T => true,
        Value::Int(number) => *number == wanted,
        Value::List(items) => items.contains(&Value::Int(wanted)),
        _ => false,
    }
}

fn int(value: &Value) -> Option<i64> {
    match value {
        Value::Int(number) => Some(*number),
        _ => None,
    }
}

fn entry(scope: &Scope, applies: bool) -> Value {
    if applies {
        Value::Entry(scope.entry.to_string())
    } else {
        Value::Nil
    }
}

/// `diary-date` with the arguments in American order.
fn date(scope: &Scope, month: &Value, day: &Value, year: &Value) -> Value {
    let date = scope.date;
    entry(scope, matches(day, date.day) && matches(month, date.month) && matches(year, date.year))
}

/// `diary-block`, American order.
fn block(scope: &Scope, first: (i64, i64, i64), last: (i64, i64, i64)) -> Value {
    let (start, end) = (absolute(first.0, first.1, first.2), absolute(last.0, last.1, last.2));
    entry(scope, start <= scope.day && scope.day <= end)
}

/// `diary-anniversary`, American order.
fn anniversary(scope: &Scope, month: i64, day: i64, year: Option<i64>) -> Value {
    let date = scope.date;
    let diff = year.map_or(100, |year| date.year - year);
    let leap = days_in_month(date.year, 2) == 29;
    let (month, day) = if month == 2 && day == 29 && !leap { (3, 1) } else { (month, day) };
    if diff > 0 && month == date.month && day == date.day {
        Value::Entry(format_count(scope.entry, diff))
    } else {
        Value::Nil
    }
}

/// `diary-cyclic`, American order.
fn cyclic(scope: &Scope, n: i64, month: i64, day: i64, year: i64) -> Option<Value> {
    if n <= 0 {
        return None;
    }
    let diff = scope.day - absolute(month, day, year);
    Some(if diff >= 0 && diff % n == 0 { Value::Entry(format_count(scope.entry, diff / n)) } else { Value::Nil })
}

/// `diary-float`
fn float(scope: &Scope, month: &Value, dayname: i64, n: i64, day: Option<i64>) -> Value {
    let date = scope.date;
    if dayname != weekday(scope.day) as i64 {
        return Value::Nil;
    }
    let limit = nth_named_absday(-n, dayname, date.month, date.year, Some(date.day));
    let last_abs = if n > 0 { limit } else { limit + 6 };
    let first_abs = if n > 0 { limit - 6 } else { limit };
    let (first, last) = (Date::from_day(first_abs), Date::from_day(last_abs));
    let base = |at: Date| day.unwrap_or_else(|| if n > 0 { 1 } else { days_in_month(at.year, at.month) });
    let applies = if first.month == last.month {
        matches(month, first.month) && first.day <= base(first) && base(first) <= last.day
    } else {
        (first.year < last.year || (first.year == last.year && first.month < last.month))
            && ((matches(month, first.month) && first.day <= base(first))
                || (matches(month, last.month) && base(last) <= last.day))
    };
    entry(scope, applies)
}

/// `org-class`: between the dates, on `dayname`, but for the ISO weeks of
/// `skip` (a holiday skip is not known here, and skips nothing).
fn class(scope: &Scope, first: (i64, i64, i64), last: (i64, i64, i64), dayname: i64, skip: &[Value]) -> Value {
    let (start, end) = (absolute(first.1, first.2, first.0), absolute(last.1, last.2, last.0));
    let week = crate::date::iso_week(scope.day).0;
    let skipped = skip.contains(&Value::Int(week));
    // The entry itself, a string, which `org-diary-sexp-entry` splits at
    // `; ` as it splits any string.
    if start <= scope.day && scope.day <= end && weekday(scope.day) as i64 == dayname && !skipped {
        Value::Str(scope.entry.to_string())
    } else {
        Value::Nil
    }
}

fn evaluate(form: &Value, scope: &Scope) -> Option<Value> {
    let Value::List(items) = form else {
        return match form {
            Value::Symbol(name) if name == "date" => Some(Value::List(vec![
                Value::Int(scope.date.month),
                Value::Int(scope.date.day),
                Value::Int(scope.date.year),
            ])),
            Value::Symbol(name) if name == "entry" => Some(Value::Str(scope.entry.to_string())),
            Value::Symbol(_) => None,
            other => Some(other.clone()),
        };
    };
    let (head, rest) = items.split_first()?;
    let Value::Symbol(name) = head else { return None };
    match name.as_str() {
        "quote" => return rest.first().cloned(),
        "and" => {
            let mut last = Value::T;
            for item in rest {
                last = evaluate(item, scope)?;
                if !last.truthy() {
                    return Some(Value::Nil);
                }
            }
            return Some(last);
        }
        "or" => {
            for item in rest {
                let value = evaluate(item, scope)?;
                if value.truthy() {
                    return Some(value);
                }
            }
            return Some(Value::Nil);
        }
        _ => {}
    }
    let arguments: Vec<Value> = rest.iter().map(|item| evaluate(item, scope)).collect::<Option<_>>()?;
    let number = |index: usize| arguments.get(index).and_then(int);
    let optional = |index: usize| arguments.get(index).filter(|value| **value != Value::Nil).and_then(int);
    let boolean = |on: bool| if on { Value::T } else { Value::Nil };
    Some(match name.as_str() {
        "not" | "null" => boolean(!arguments.first()?.truthy()),
        "=" | "<" | ">" | "<=" | ">=" | "/=" => {
            let numbers: Vec<i64> = arguments.iter().map(int).collect::<Option<_>>()?;
            boolean(numbers.windows(2).all(|pair| match name.as_str() {
                "=" => pair[0] == pair[1],
                "<" => pair[0] < pair[1],
                ">" => pair[0] > pair[1],
                "<=" => pair[0] <= pair[1],
                ">=" => pair[0] >= pair[1],
                _ => pair[0] != pair[1],
            }))
        }
        "memq" | "member" => match arguments.get(1)? {
            Value::List(items) => boolean(items.contains(arguments.first()?)),
            _ => Value::Nil,
        },
        "calendar-day-of-week" => Value::Int(weekday(scope.day) as i64),
        "calendar-extract-month" => Value::Int(scope.date.month),
        "calendar-extract-day" => Value::Int(scope.date.day),
        "calendar-extract-year" => Value::Int(scope.date.year),
        "diary-date" => date(scope, arguments.first()?, arguments.get(1)?, arguments.get(2)?),
        "org-date" => date(scope, arguments.get(1)?, arguments.get(2)?, arguments.first()?),
        "diary-block" => block(scope, (number(0)?, number(1)?, number(2)?), (number(3)?, number(4)?, number(5)?)),
        "org-block" => block(scope, (number(1)?, number(2)?, number(0)?), (number(4)?, number(5)?, number(3)?)),
        "diary-anniversary" => anniversary(scope, number(0)?, number(1)?, optional(2)),
        "org-anniversary" => anniversary(scope, number(1)?, number(2)?, Some(number(0)?)),
        "diary-cyclic" => cyclic(scope, number(0)?, number(1)?, number(2)?, number(3)?)?,
        "org-cyclic" => cyclic(scope, number(0)?, number(2)?, number(3)?, number(1)?)?,
        "diary-float" => float(scope, arguments.first()?, number(1)?, number(2)?, optional(3)),
        "org-class" => class(
            scope,
            (number(0)?, number(1)?, number(2)?),
            (number(3)?, number(4)?, number(5)?),
            number(6)?,
            &arguments[7.min(arguments.len())..],
        ),
        _ => return None,
    })
}

/// What another evaluator, Emacs's, makes of a diary sexp this one cannot
/// read: `(sexp, entry, day, place)` to the lines it shows, none when it
/// does not apply.
#[derive(Clone, Default)]
pub struct Outside(pub Option<std::sync::Arc<Evaluate>>);

/// `(sexp, entry, day, place)` to the lines shown, none when the sexp does
/// not apply. `place` is where the sexp is, for what Emacs says of one
/// that fails.
pub type Evaluate = dyn Fn(&str, &str, Day, &Place) -> Option<Vec<String>> + Send + Sync;

/// Where a diary sexp is: its file's path, when known, and its line,
/// from 1.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Place {
    pub file: Option<String>,
    pub line: usize,
}

impl std::fmt::Debug for Outside {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str(if self.0.is_some() { "Outside(Some)" } else { "Outside(None)" })
    }
}

/// The settings are equal whoever evaluates what this one cannot.
impl PartialEq for Outside {
    fn eq(&self, _: &Outside) -> bool {
        true
    }
}

impl Eq for Outside {}

/// `org-diary-sexp-entry`: what the diary sexp `sexp` (with its `(` and
/// `)`) with the text `entry` gives on `day`: none when it does not apply,
/// else the lines it shows. One this evaluator cannot read goes to
/// `outside`, when there is one, with its `place`; without it, it does not
/// apply.
pub fn entries_with(sexp: &str, entry: &str, day: Day, outside: &Outside, place: &Place) -> Option<Vec<String>> {
    match evaluated(sexp, entry, day) {
        Some(result) => result,
        None => outside.0.as_ref().and_then(|outside| outside(sexp, entry, day, place)),
    }
}

/// [`entries_with`], with nothing outside.
pub fn entries(sexp: &str, entry: &str, day: Day) -> Option<Vec<String>> {
    entries_with(sexp, entry, day, &Outside::default(), &Place::default())
}

/// What this evaluator makes of the sexp: none when it cannot read it.
fn evaluated(sexp: &str, entry: &str, day: Day) -> Option<Option<Vec<String>>> {
    let form = read(sexp)?;
    let scope = Scope { date: Date::from_day(day), day, entry };
    Some(match evaluate(&form, &scope)? {
        Value::Nil => None,
        Value::Str(text) => Some(text.split("; ").map(str::to_string).collect()),
        Value::Entry(text) => Some(vec![text]),
        Value::List(items) if items.is_empty() => None,
        Value::List(items) if matches!(items.first(), Some(Value::Str(_))) => Some(
            items
                .into_iter()
                .filter_map(|item| match item {
                    Value::Str(text) => Some(text),
                    _ => None,
                })
                .collect(),
        ),
        _ => Some(vec![entry.to_string()]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(text: &str) -> Day {
        crate::timestr::time_string_to_day(&format!("<{text}>")).unwrap()
    }

    #[test]
    fn evaluates_the_date_functions() {
        let shown = |sexp: &str, entry: &str, day: &str| entries(sexp, entry, on(day));
        assert_eq!(
            shown("(diary-anniversary 5 18 1990)", "Ann is %d years old", "2026-05-18"),
            Some(vec!["Ann is 36 years old".to_string()])
        );
        assert_eq!(
            shown("(org-anniversary 1990 5 18)", "%d%s birthday", "2026-05-18"),
            Some(vec!["36th birthday".to_string()])
        );
        assert_eq!(shown("(diary-anniversary 5 18 1990)", "x", "2026-05-19"), None);
        assert_eq!(shown("(diary-anniversary 2 29 2000)", "leap", "2026-03-01"), Some(vec!["leap".to_string()]));
        assert_eq!(shown("(diary-block 5 10 2026 5 20 2026)", "Trip", "2026-05-18"), Some(vec!["Trip".to_string()]));
        assert_eq!(shown("(org-block 2026 5 10 2026 5 17)", "Trip", "2026-05-18"), None);
        assert_eq!(shown("(diary-cyclic 7 5 4 2026)", "%d%s week", "2026-05-18"), Some(vec!["2nd week".to_string()]));
        // The fourth Monday of May 2026 is the 25th, the second the 11th.
        assert_eq!(shown("(diary-float 5 1 4)", "x", "2026-05-25"), Some(vec!["x".to_string()]));
        assert_eq!(shown("(diary-float t 1 2)", "x", "2026-05-11"), Some(vec!["x".to_string()]));
        assert_eq!(shown("(diary-float t 1 2)", "x", "2026-05-18"), None);
        // The last Monday of May.
        assert_eq!(shown("(diary-float 5 1 -1)", "x", "2026-05-25"), Some(vec!["x".to_string()]));
        assert_eq!(shown("(diary-date t 18 t)", "x", "2026-05-18"), Some(vec!["x".to_string()]));
        assert_eq!(shown("(diary-date '(4 5) 18 2026)", "x", "2026-05-18"), Some(vec!["x".to_string()]));
        assert_eq!(
            shown("(and (= 1 (calendar-day-of-week date)) (diary-block 5 1 2026 5 31 2026))", "Mondays", "2026-05-18"),
            Some(vec!["Mondays".to_string()])
        );
        assert_eq!(shown("(org-class 2026 5 1 2026 6 30 1 21)", "Class", "2026-05-18"), None);
        assert_eq!(shown("(org-class 2026 5 1 2026 6 30 1)", "Class", "2026-05-18"), Some(vec!["Class".to_string()]));
        // A string, which `org-diary-sexp-entry` splits.
        assert_eq!(
            shown("(org-class 2026 5 1 2026 6 30 1)", "Lecture; Room 5", "2026-05-18"),
            Some(vec!["Lecture".to_string(), "Room 5".to_string()])
        );
        assert_eq!(shown("(my-own-function)", "x", "2026-05-18"), None);
        assert_eq!(shown("\"a; b\"", "x", "2026-05-18"), Some(vec!["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn does_not_read_a_form_nested_past_the_stack() {
        assert_eq!(read(&"(".repeat(100_000)), None);
    }
}
