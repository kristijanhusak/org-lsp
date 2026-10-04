//! The agenda: which entries show on which day, in what order, and how.
//!
//! The rules are those of `org-agenda.el`. [`day_items`] ports
//! `org-agenda-get-day-entries` with its four sources (deadlines,
//! scheduled items, date ranges, plain timestamps), [`todo_items`] ports
//! `org-agenda-get-todos`, and [`view`] lays the result out the way
//! `org-agenda-list` and `org-todo-list` do. The output is plain text plus
//! enough structure for an editor to highlight it and to find the entry
//! behind each line.

mod item;

pub use crate::text::tag_group;
pub use item::{align_tags, display_column, hidden_ranges, Face, Item, ItemKind, Source};

use item::{format_item, normalise_keyword, priority, Dotime, FormatInput, Prefix};

use crate::date::{days_in_month, is_leap_year, iso_week, weekday, Date, Day, MONTH_NAMES, WEEKDAY_NAMES};
use crate::outline::{Entry, Outline};
use crate::timestr::{closest_date, time_string_to_day, warning_days, Prefer};

/// How many days the agenda covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    Day,
    Week,
    Fortnight,
    Month,
    Year,
    Days(i64),
}

impl Span {
    /// Whole numbers of days that have a name are that span.
    pub fn from_days(days: i64) -> Span {
        match days {
            1 => Span::Day,
            7 => Span::Week,
            14 => Span::Fortnight,
            days => Span::Days(days.max(1)),
        }
    }

    /// The number of days, for a span that starts on `start`.
    pub fn days(self, start: Day) -> i64 {
        let date = Date::from_day(start);
        match self {
            Span::Day => 1,
            Span::Week => 7,
            Span::Fortnight => 14,
            Span::Month => days_in_month(date.year, date.month),
            Span::Year => {
                if is_leap_year(date.year) {
                    366
                } else {
                    365
                }
            }
            Span::Days(days) => days,
        }
    }

    fn name(self) -> String {
        match self {
            Span::Day => "Day".to_string(),
            Span::Week => "Week".to_string(),
            Span::Fortnight => "Fortnight".to_string(),
            Span::Month => "Month".to_string(),
            Span::Year => "Year".to_string(),
            Span::Days(days) => format!("{days} days"),
        }
    }

    /// The start of the span that follows (`steps` > 0) or precedes this one.
    pub fn shift(self, start: Day, steps: i64) -> Day {
        let date = Date::from_day(start);
        match self {
            Span::Day => start + steps,
            Span::Week => start + 7 * steps,
            Span::Fortnight => start + 14 * steps,
            Span::Days(days) => start + days * steps,
            Span::Month => Date::new(date.year, date.month + steps, date.day).to_day(),
            Span::Year => Date::new(date.year + steps, date.month, date.day).to_day(),
        }
    }
}

/// Which occurrences of a repeating entry show on days after today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FutureRepeats {
    All,
    /// Only the next one.
    Next,
    None,
}

/// When and how the time grid is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeGrid {
    /// In a one-day agenda.
    pub daily: bool,
    /// For today, whatever the span.
    pub today: bool,
    /// For every day.
    pub weekly: bool,
    /// Only on days that have an entry with a time.
    pub require_timed: bool,
    /// Leave out grid lines at a time an entry already has.
    pub remove_match: bool,
    /// Military times, such as 800 and 1000.
    pub times: Vec<i64>,
    /// What follows a time that has no end time.
    pub trailer: String,
    /// The text of a grid line.
    pub line: String,
    /// The text of the line for the current time.
    pub now: String,
}

impl Default for TimeGrid {
    fn default() -> Self {
        TimeGrid {
            daily: true,
            today: true,
            weekly: false,
            require_timed: true,
            remove_match: false,
            times: vec![800, 1000, 1200, 1400, 1600, 1800, 2000],
            trailer: "......".to_string(),
            line: "----------------".to_string(),
            now: "now - - - - - - - - - - - - - - - - - - - - - - - - -".to_string(),
        }
    }
}

/// Agenda settings. The names and defaults are Org's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `org-agenda-span`
    pub span: Span,
    /// `org-agenda-start-on-weekday`: 0 is Sunday. Applies to week and
    /// fortnight views.
    pub start_on_weekday: Option<usize>,
    /// `org-deadline-warning-days`
    pub deadline_warning_days: i64,
    /// `org-scheduled-delay-days`
    pub scheduled_delay_days: i64,
    /// `org-scheduled-past-days`
    pub scheduled_past_days: i64,
    /// `org-deadline-past-days`
    pub deadline_past_days: i64,
    /// `org-agenda-skip-scheduled-if-done`
    pub skip_scheduled_if_done: bool,
    /// `org-agenda-skip-deadline-if-done`
    pub skip_deadline_if_done: bool,
    /// `org-agenda-skip-timestamp-if-done`
    pub skip_timestamp_if_done: bool,
    /// `org-agenda-show-future-repeats`
    pub show_future_repeats: FutureRepeats,
    /// `org-agenda-show-all-dates`
    pub show_all_dates: bool,
    /// `org-agenda-tags-column`. Org's default follows the window width;
    /// a server does not know it, so the default is -80.
    pub tags_column: i64,
    /// `org-agenda-use-time-grid`
    pub use_time_grid: bool,
    /// `org-agenda-time-grid` and `org-agenda-current-time-string`
    pub time_grid: TimeGrid,
    /// `org-agenda-show-current-time-in-grid`
    pub show_current_time_in_grid: bool,
    /// `org-agenda-weekend-days`
    pub weekend_days: Vec<usize>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            span: Span::Week,
            start_on_weekday: Some(1),
            deadline_warning_days: 14,
            scheduled_delay_days: 0,
            scheduled_past_days: 10_000,
            deadline_past_days: 10_000,
            skip_scheduled_if_done: false,
            skip_deadline_if_done: false,
            skip_timestamp_if_done: false,
            show_future_repeats: FutureRepeats::All,
            show_all_dates: true,
            tags_column: -80,
            use_time_grid: true,
            time_grid: TimeGrid::default(),
            show_current_time_in_grid: true,
            weekend_days: vec![6, 0],
        }
    }
}

/// The moment the agenda is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    pub today: Day,
    /// Minutes since midnight.
    pub minute: i64,
}

struct Builder<'a> {
    files: &'a [&'a Outline],
    config: &'a Config,
    today: Day,
}

impl Builder<'_> {
    /// The date a repeating timestamp shows on when `current` is after
    /// today; before that, it only shows at `base`.
    fn future_repeat(&self, raw: &str, base: Day, current: Day) -> Day {
        if current <= self.today {
            return base;
        }
        let from = match self.config.show_future_repeats {
            FutureRepeats::None => return base,
            FutureRepeats::Next => self.today + 1,
            FutureRepeats::All => current,
        };
        closest_date(raw, from, Prefer::Future).unwrap_or(base)
    }

    fn item(
        &self,
        file: usize,
        entry_index: usize,
        kind: ItemKind,
        face: Face,
        extra: &str,
        dotime: Dotime<'_>,
    ) -> Item {
        let outline = self.files[file];
        let entry = &outline.entries[entry_index];
        let formatted = format_item(&FormatInput {
            prefix: if kind == ItemKind::Todo { Prefix::Todo } else { Prefix::Agenda },
            extra,
            head: &entry.head,
            category: &entry.category,
            tags: &entry.tags,
            dotime,
            remove_stamps: kind == ItemKind::Timestamp,
            time_trailer: &self.config.time_grid.trailer,
        });
        let mut text = formatted.text;
        let priority = priority(&text);
        let keyword_len = normalise_keyword(&mut text, formatted.heading, outline.todo.names()).unwrap_or(0);
        let keyword_done = outline.todo.is_done(&text[formatted.heading..formatted.heading + keyword_len]);
        Item {
            text,
            heading: formatted.heading,
            time_of_day: formatted.time_of_day,
            priority,
            kind,
            face,
            source: Some(Source { file, entry: entry_index }),
            keyword_len,
            keyword_done,
        }
    }

    /// `org-agenda-get-deadlines`
    fn deadlines(&self, file: usize, current: Day, out: &mut Vec<Item>) {
        let is_today = current == self.today;
        for (index, entry) in self.files[file].entries.iter().enumerate() {
            let found = entry.deadline.as_ref().filter(|planning| planning.active && planning.searchable);
            let Some(planning) = found.filter(|_| !entry.skip) else {
                continue;
            };
            let raw = planning.raw.as_str();
            let Some(deadline) = time_string_to_day(raw) else { continue };
            let repeat = self.future_repeat(raw, deadline, current);
            let diff = deadline - current;
            let wdays = warning_days(raw, self.config.deadline_warning_days, false, false);
            // A deadline shows on its date, on later repeats, and today
            // while it is within its warning period or overdue.
            let shown = current == deadline
                || current == repeat
                || (is_today
                    && if deadline > current { diff <= wdays } else { -diff <= self.config.deadline_past_days });
            if !shown || (entry.element_done && (self.config.skip_deadline_if_done || deadline != current)) {
                continue;
            }
            let extra = if is_today && deadline < self.today {
                format!("{:2} d. ago: ", -diff)
            } else if is_today && deadline > self.today {
                format!("In {diff:3} d.: ")
            } else {
                "Deadline:  ".to_string()
            };
            let fraction = 1.0 - diff as f64 / wdays.max(1) as f64;
            let face = if entry.element_done {
                Face::Done
            } else if fraction >= 1.0 {
                Face::DeadlineWarning
            } else if fraction >= 0.5 {
                Face::DeadlineUpcoming
            } else {
                Face::DeadlineDistant
            };
            let time = planning_time(raw, current != deadline && current != repeat);
            let mut item = self.item(file, index, ItemKind::Deadline, face, &extra, time.as_dotime());
            // Overdue deadlines sort first, then the most imminent ones.
            item.priority += if is_today { -diff } else { 0 };
            out.push(item);
        }
    }

    /// `org-agenda-get-scheduled`
    fn scheduled(&self, file: usize, current: Day, out: &mut Vec<Item>) {
        let is_today = current == self.today;
        for (index, entry) in self.files[file].entries.iter().enumerate() {
            let found = entry.scheduled.as_ref().filter(|planning| planning.active && planning.searchable);
            let Some(planning) = found.filter(|_| !entry.skip) else {
                continue;
            };
            let raw = planning.raw.as_str();
            let Some(schedule) = time_string_to_day(raw) else { continue };
            let repeat = self.future_repeat(raw, schedule, current);
            let diff = current - schedule;
            let past = schedule < self.today;
            let delay = warning_days(raw, self.config.scheduled_delay_days, true, false);
            // A scheduled entry shows on its date, today once that date has
            // passed, and on later repeats; a delay holds it back.
            let hidden = (delay > 0 && diff < delay)
                || diff > self.config.scheduled_past_days
                || schedule > current
                || (current != schedule && !is_today && current != repeat);
            if hidden || (entry.element_done && (self.config.skip_scheduled_if_done || schedule != current)) {
                continue;
            }
            let extra = if is_today && past { format!("Sched.{diff:2}x: ") } else { "Scheduled: ".to_string() };
            let face = if entry.element_done {
                Face::Done
            } else if past {
                Face::ScheduledPreviously
            } else if is_today {
                Face::ScheduledToday
            } else {
                Face::Scheduled
            };
            let time = planning_time(raw, current != schedule && current != repeat);
            let mut item = self.item(file, index, ItemKind::Scheduled, face, &extra, time.as_dotime());
            item.priority += 99 + diff;
            out.push(item);
        }
    }

    /// `org-agenda-get-blocks`
    fn blocks(&self, file: usize, current: Day, out: &mut Vec<Item>) {
        let outline = self.files[file];
        for block in &outline.blocks {
            let entry = &outline.entries[block.entry];
            let (Some(first), Some(last)) = (time_string_to_day(&block.first), time_string_to_day(&block.second))
            else {
                continue;
            };
            if current < first || current > last || (entry.done && self.config.skip_timestamp_if_done) {
                continue;
            }
            let (extra, face) = if first == last {
                (String::new(), Face::CalendarEvent)
            } else {
                (format!("({}/{}): ", 1 + current - first, 1 + last - first), Face::CalendarDateRange)
            };
            // Times count on the first and on the last day.
            let dotime = match (first == current, last == current) {
                (true, true) => Some(format!("<{}>--<{}>", block.first, block.second)),
                (true, false) => Some(format!("<{}>", block.first)),
                (false, true) => Some(format!("<{}>", block.second)),
                (false, false) => None,
            };
            let dotime = dotime.as_deref().map_or(Dotime::No, Dotime::Text);
            out.push(self.item(file, block.entry, ItemKind::Block, face, &extra, dotime));
        }
    }

    /// `org-agenda-get-timestamps`
    fn timestamps(&self, file: usize, current: Day, out: &mut Vec<Item>) {
        let outline = self.files[file];
        let current_date = Date::from_day(current);
        // Org scans the text once per day and stops where today's date
        // or a repeater stands; the next stop comes after the last one.
        let mut scanned_to = 0;
        for stamp in &outline.stamps {
            if stamp.offset < scanned_to {
                continue;
            }
            let (shown, repeats) = if stamp.date == Some(current_date) {
                scanned_to = stamp.offset + 11;
                (&stamp.on_date, false)
            } else if let Some(end) = stamp.repeat_end {
                scanned_to = end;
                (&stamp.on_repeat, true)
            } else {
                continue;
            };
            let Some(shown) = shown else { continue };
            if outline.entries[shown.entry].done && self.config.skip_timestamp_if_done {
                continue;
            }
            if repeats {
                // Past occurrences show up to today, future ones after it.
                let base = if current > self.today { self.today } else { current };
                let Some(past) = closest_date(&shown.text, base, Prefer::Past) else { continue };
                let future = self.future_repeat(&shown.text, past, current);
                if current != past && current != future {
                    continue;
                }
            }
            out.push(self.item(
                file,
                shown.entry,
                ItemKind::Timestamp,
                Face::CalendarEvent,
                "",
                Dotime::Text(&shown.text),
            ));
        }
    }

    /// `org-agenda-get-day-entries` for one file, in Org's order.
    fn day(&self, file: usize, current: Day, out: &mut Vec<Item>) {
        self.deadlines(file, current, out);
        self.scheduled(file, current, out);
        self.blocks(file, current, out);
        self.timestamps(file, current, out);
    }

    /// `org-agenda-add-time-grid-maybe`
    fn add_time_grid(&self, items: &mut Vec<Item>, ndays: i64, is_today: bool, now_minute: i64) {
        let grid = &self.config.time_grid;
        let wanted =
            self.config.use_time_grid && ((is_today && grid.today) || (ndays == 1 && grid.daily) || grid.weekly);
        let have: Vec<i64> = items.iter().filter_map(|item| item.time_of_day).collect();
        if !wanted || (grid.require_timed && have.is_empty()) {
            return;
        }
        let line = |text: &str, time: &str, face: Face| {
            let formatted = format_item(&FormatInput {
                prefix: Prefix::Agenda,
                extra: "",
                head: text,
                category: "",
                tags: &[],
                dotime: Dotime::Text(time),
                remove_stamps: false,
                time_trailer: &grid.trailer,
            });
            Item {
                text: formatted.text,
                heading: formatted.heading,
                time_of_day: formatted.time_of_day,
                priority: 0,
                kind: ItemKind::Grid,
                face,
                source: None,
                keyword_len: 0,
                keyword_done: false,
            }
        };
        // Org builds this list back to front, and the order decides ties.
        let mut lines = Vec::new();
        if is_today && self.config.show_current_time_in_grid {
            let time = format!("{:02}:{:02} ", now_minute / 60, now_minute % 60);
            lines.push(line(&grid.now, &time, Face::CurrentTime));
        }
        for time in grid.times.iter().rev() {
            if grid.remove_match && have.contains(time) {
                continue;
            }
            let time = format!("{:02}:{:02}", time / 100, time % 100);
            lines.push(line(&grid.line, &time, Face::TimeGrid));
        }
        lines.append(items);
        *items = lines;
    }
}

/// The time part of a planning timestamp, as `org-agenda-format-item`
/// receives it.
enum PlanningTime {
    /// A reminder on another day carries no time.
    None,
    /// From the first time in the timestamp to its end, and a blank.
    Text(String),
    /// No time in the timestamp: the headline may still have one.
    Headline,
}

impl PlanningTime {
    fn as_dotime(&self) -> Dotime<'_> {
        match self {
            PlanningTime::None => Dotime::No,
            PlanningTime::Text(text) => Dotime::Text(text),
            PlanningTime::Headline => Dotime::Headline,
        }
    }
}

fn planning_time(raw: &str, reminder: bool) -> PlanningTime {
    if reminder {
        return PlanningTime::None;
    }
    // ` \([012]?[0-9]:[0-9][0-9]\)`
    let bytes = raw.as_bytes();
    let found = (0..bytes.len()).find(|at| {
        if bytes[*at] != b' ' {
            return false;
        }
        let rest = &bytes[at + 1..];
        let hour = match rest {
            [b'0'..=b'2', b'0'..=b'9', b':', ..] => 2,
            [b'0'..=b'9', b':', ..] => 1,
            _ => return false,
        };
        matches!(rest.get(hour + 1..hour + 3), Some([b'0'..=b'9', b'0'..=b'9']))
    });
    match found {
        Some(at) => PlanningTime::Text(format!("{} ", &raw[at + 1..])),
        None => PlanningTime::Headline,
    }
}

/// `org-entries-lessp` with the default strategies: by time (lines
/// without one last), then by priority, highest first.
fn sort(items: &mut [Item], by_time: bool) {
    items.sort_by(|a, b| {
        let time = |item: &Item| item.time_of_day.unwrap_or(9901);
        let by_time = if by_time { time(a).cmp(&time(b)) } else { std::cmp::Ordering::Equal };
        by_time.then(b.priority.cmp(&a.priority))
    });
}

/// The entries shown for `current`, sorted, without the time grid.
pub fn day_items(files: &[&Outline], current: Day, today: Day, config: &Config) -> Vec<Item> {
    let builder = Builder { files, config, today };
    let mut items = Vec::new();
    for file in 0..files.len() {
        builder.day(file, current, &mut items);
    }
    sort(&mut items, true);
    items
}

/// `org-agenda-get-todos` over all files: every entry in a not-done state.
pub fn todo_items(files: &[&Outline], config: &Config) -> Vec<Item> {
    let builder = Builder { files, config, today: 0 };
    let mut items = Vec::new();
    for (file, outline) in files.iter().enumerate() {
        for (index, entry) in outline.entries.iter().enumerate() {
            if entry.skip || !entry.is_open_todo() {
                continue;
            }
            let mut item = builder.item(file, index, ItemKind::Todo, Face::Default, "", Dotime::Headline);
            item.priority += 1;
            items.push(item);
        }
    }
    sort(&mut items, false);
    items
}

/// What a line of a view is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// The title of the view.
    Header,
    /// A day heading.
    Date { day: Day, today: bool, weekend: bool },
    /// An entry or a grid line.
    Item,
}

/// One line of a rendered view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub kind: LineKind,
    /// The item behind an `Item` line.
    pub item: Option<Item>,
}

/// A rendered agenda.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct View {
    pub lines: Vec<Line>,
    /// The first day shown, for moving to the next or previous span.
    pub start: Day,
}

impl View {
    pub fn text(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(&line.text);
            out.push('\n');
        }
        out
    }
}

fn item_line(item: Item, config: &Config) -> Line {
    Line { text: align_tags(&item.text, config.tags_column), kind: LineKind::Item, item: Some(item) }
}

/// The first day of an agenda that should include `day`: week views go
/// back to the configured weekday.
pub fn span_start(day: Day, span: Span, config: &Config) -> Day {
    let ndays = span.days(day);
    match config.start_on_weekday {
        Some(first) if ndays == 7 || ndays == 14 => {
            let back = (weekday(day) as i64 - first as i64).rem_euclid(7);
            day - back
        }
        _ => day,
    }
}

/// `org-agenda-format-date-aligned`
fn date_heading(day: Day) -> String {
    let date = Date::from_day(day);
    let week = if weekday(day) == 1 { format!(" W{:02}", iso_week(day).0) } else { String::new() };
    format!(
        "{:<10} {:2} {} {:4}{week}",
        WEEKDAY_NAMES[weekday(day)],
        date.day,
        MONTH_NAMES[(date.month - 1) as usize],
        date.year
    )
}

/// `org-agenda-list`: the day or week agenda. `start_day` is the day the
/// view should include; `None` means today.
pub fn view(files: &[&Outline], now: Now, start_day: Option<Day>, span: Span, config: &Config) -> View {
    let builder = Builder { files, config, today: now.today };
    let start = span_start(start_day.unwrap_or(now.today), span, config);
    let ndays = span.days(start_day.unwrap_or(now.today));
    let last = start + ndays - 1;

    let (first_week, last_week) = (iso_week(start).0, iso_week(last).0);
    let weeks = if last - start >= 350 {
        String::new()
    } else if first_week == last_week {
        format!(" (W{first_week:02})")
    } else {
        format!(" (W{first_week:02}-W{last_week:02})")
    };
    let mut lines = vec![Line { text: format!("{}-agenda{weeks}:", span.name()), kind: LineKind::Header, item: None }];

    for day in start..=last {
        let is_today = day == now.today;
        let mut items = Vec::new();
        for file in 0..files.len() {
            builder.day(file, day, &mut items);
        }
        if items.is_empty() && !config.show_all_dates {
            continue;
        }
        lines.push(Line {
            text: date_heading(day),
            kind: LineKind::Date { day, today: is_today, weekend: config.weekend_days.contains(&weekday(day)) },
            item: None,
        });
        builder.add_time_grid(&mut items, ndays, is_today, now.minute);
        sort(&mut items, true);
        lines.extend(items.into_iter().map(|item| item_line(item, config)));
    }
    View { lines, start }
}

/// `org-todo-list`: the global list of open TODO entries.
pub fn todo_view(files: &[&Outline], config: &Config) -> View {
    let mut lines =
        vec![Line { text: "Global list of TODO items of type: ALL".to_string(), kind: LineKind::Header, item: None }];
    lines.extend(todo_items(files, config).into_iter().map(|item| item_line(item, config)));
    View { lines, start: 0 }
}

/// The entry an agenda line was made from.
pub fn entry<'a>(files: &[&'a Outline], source: Source) -> &'a Entry {
    &files[source.file].entries[source.entry]
}
