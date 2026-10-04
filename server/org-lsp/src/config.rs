//! Settings a client passes as `initializationOptions`, and the command
//! line passes as JSON. Names are Org's variable names in camel case
//! without the `org-agenda-` prefix.

use org_core::agenda::{Config as AgendaConfig, FutureRepeats, Span, TimeGrid};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    /// The default TODO sequence, written like Org's `#+TODO:` line:
    /// `["TODO", "NEXT", "|", "DONE"]`. Documents can override it.
    pub todo_keywords: Option<Vec<String>>,
    /// Files, directories and glob patterns the agenda is built from.
    pub agenda_files: Vec<String>,
    pub agenda: AgendaOptions,
    /// Whether the client fetches the agenda from the server as a virtual
    /// document. Clients that announce LSP 3.18's `textDocumentContent`
    /// need not say so.
    pub virtual_documents: Option<bool>,
    /// A fixed date and time, for tests.
    pub clock: Option<ClockOption>,
}

/// `{ "today": "2026-05-18", "now": "10:30" }`
#[derive(Debug, Clone, Deserialize)]
pub struct ClockOption {
    pub today: String,
    pub now: Option<String>,
}

impl ClockOption {
    pub fn to_now(&self) -> Option<org_core::agenda::Now> {
        let today = crate::cli::parse_date(&self.today).ok()?;
        let minute = match &self.now {
            Some(now) => crate::cli::parse_time(now).ok()?,
            None => 0,
        };
        Some(org_core::agenda::Now { today, minute })
    }
}

/// A span given as a name or as a number of days.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SpanOption {
    Name(String),
    Days(i64),
}

impl SpanOption {
    pub fn to_span(&self) -> Option<Span> {
        match self {
            SpanOption::Days(days) if *days > 0 => Some(Span::from_days(*days)),
            SpanOption::Days(_) => None,
            SpanOption::Name(name) => Some(match name.as_str() {
                "day" => Span::Day,
                "week" => Span::Week,
                "fortnight" => Span::Fortnight,
                "month" => Span::Month,
                "year" => Span::Year,
                other => return other.parse().ok().filter(|days| *days > 0).map(Span::from_days),
            }),
        }
    }
}

/// `org-agenda-show-future-repeats`: `true`, `false` or `"next"`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum FutureRepeatsOption {
    Flag(bool),
    Name(String),
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TimeGridOptions {
    pub daily: Option<bool>,
    pub today: Option<bool>,
    pub weekly: Option<bool>,
    pub require_timed: Option<bool>,
    pub remove_match: Option<bool>,
    pub times: Option<Vec<i64>>,
    pub trailer: Option<String>,
    pub line: Option<String>,
    pub now: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgendaOptions {
    pub span: Option<SpanOption>,
    /// 0 is Sunday; `false` starts week views on the current day.
    pub start_on_weekday: Option<serde_json::Value>,
    pub deadline_warning_days: Option<i64>,
    pub scheduled_delay_days: Option<i64>,
    pub scheduled_past_days: Option<i64>,
    pub deadline_past_days: Option<i64>,
    pub skip_scheduled_if_done: Option<bool>,
    pub skip_deadline_if_done: Option<bool>,
    pub skip_timestamp_if_done: Option<bool>,
    pub show_future_repeats: Option<FutureRepeatsOption>,
    pub show_all_dates: Option<bool>,
    pub tags_column: Option<i64>,
    pub use_time_grid: Option<bool>,
    pub time_grid: TimeGridOptions,
    pub show_current_time_in_grid: Option<bool>,
    pub weekend_days: Option<Vec<usize>>,
}

impl AgendaOptions {
    /// Org's defaults with the given settings applied. Values that make
    /// no sense are ignored.
    pub fn to_config(&self) -> AgendaConfig {
        let mut config = AgendaConfig::default();
        macro_rules! set {
            ($($field:ident),*) => {$(
                if let Some(value) = &self.$field {
                    config.$field = value.clone();
                }
            )*};
        }
        set!(
            deadline_warning_days,
            scheduled_delay_days,
            scheduled_past_days,
            deadline_past_days,
            skip_scheduled_if_done,
            skip_deadline_if_done,
            skip_timestamp_if_done,
            show_all_dates,
            tags_column,
            use_time_grid,
            show_current_time_in_grid
        );
        if let Some(span) = self.span.as_ref().and_then(SpanOption::to_span) {
            config.span = span;
        }
        match &self.start_on_weekday {
            Some(serde_json::Value::Bool(false)) => config.start_on_weekday = None,
            Some(serde_json::Value::Number(day)) => {
                if let Some(day) = day.as_u64().filter(|day| *day < 7) {
                    config.start_on_weekday = Some(day as usize);
                }
            }
            _ => {}
        }
        match &self.show_future_repeats {
            Some(FutureRepeatsOption::Flag(true)) => config.show_future_repeats = FutureRepeats::All,
            Some(FutureRepeatsOption::Flag(false)) => config.show_future_repeats = FutureRepeats::None,
            Some(FutureRepeatsOption::Name(name)) if name == "next" => config.show_future_repeats = FutureRepeats::Next,
            _ => {}
        }
        if let Some(days) = &self.weekend_days {
            config.weekend_days = days.iter().copied().filter(|day| *day < 7).collect();
        }
        let grid = &self.time_grid;
        let target: &mut TimeGrid = &mut config.time_grid;
        macro_rules! set_grid {
            ($($field:ident),*) => {$(
                if let Some(value) = &grid.$field {
                    target.$field = value.clone();
                }
            )*};
        }
        set_grid!(daily, today, weekly, require_timed, remove_match, times, trailer, line, now);
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_orgs() {
        let options: Options = serde_json::from_str("{}").unwrap();
        assert_eq!(options.agenda.to_config(), AgendaConfig::default());
    }

    #[test]
    fn reads_settings() {
        let options: Options = serde_json::from_value(serde_json::json!({
            "agendaFiles": ["~/org"],
            "agenda": {
                "span": "day",
                "startOnWeekday": 0,
                "skipScheduledIfDone": true,
                "showFutureRepeats": "next",
                "tagsColumn": -100,
                "timeGrid": { "times": [900, 1700], "weekly": true },
                "unknown": 1
            }
        }))
        .unwrap();
        let config = options.agenda.to_config();
        assert_eq!(options.agenda_files, ["~/org"]);
        assert_eq!(config.span, Span::Day);
        assert_eq!(config.start_on_weekday, Some(0));
        assert!(config.skip_scheduled_if_done);
        assert_eq!(config.show_future_repeats, FutureRepeats::Next);
        assert_eq!(config.tags_column, -100);
        assert_eq!(config.time_grid.times, [900, 1700]);
        assert!(config.time_grid.weekly && config.time_grid.today);
    }

    #[test]
    fn spans() {
        let span = |value: serde_json::Value| serde_json::from_value::<SpanOption>(value).unwrap().to_span();
        assert_eq!(span(serde_json::json!(7)), Some(Span::Week));
        assert_eq!(span(serde_json::json!(3)), Some(Span::Days(3)));
        assert_eq!(span(serde_json::json!("10")), Some(Span::Days(10)));
        assert_eq!(span(serde_json::json!("month")), Some(Span::Month));
        assert_eq!(span(serde_json::json!(0)), None);
        assert_eq!(span(serde_json::json!("soon")), None);
        let off: AgendaOptions = serde_json::from_value(serde_json::json!({ "startOnWeekday": false })).unwrap();
        assert_eq!(off.to_config().start_on_weekday, None);
    }
}
