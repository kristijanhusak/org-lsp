//! The agenda: which entries show on which day, in what order, and how.
//!
//! The rules are those of `org-agenda.el`. [`day_items`] ports
//! `org-agenda-get-day-entries` with its four sources (deadlines,
//! scheduled items, date ranges, plain timestamps), [`todo_items`] ports
//! `org-agenda-get-todos`, and [`view`] lays the result out the way
//! `org-agenda-list` and `org-todo-list` do. The output is plain text plus
//! enough structure for an editor to highlight it and to find the entry
//! behind each line.

mod display;
mod item;
pub mod matcher;
pub mod report;
pub mod search;

pub use search::SearchSettings;

pub use crate::text::tag_group;
pub use display::{display, Displayed, Link, Segment, Style};
pub use item::{align_tags, display_column, hidden_ranges, Face, Item, ItemKind, Source};
pub use matcher::Matcher;

use item::{format_item, normalise_keyword, priority, Dotime, FormatInput};
pub use item::{format_numbers, format_with, PrefixFormat, RemoveTags};

use crate::date::{days_in_month, is_leap_year, iso_week, weekday, Date, Day, MONTH_NAMES, WEEKDAY_NAMES};
use crate::outline::{Entry, LogKind, Outline};
use crate::priority::Priorities;
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

/// `org-agenda-prefix-format`: the prefix of the lines of each kind of
/// view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixFormats {
    pub agenda: String,
    pub todo: String,
    pub tags: String,
    pub search: String,
}

impl Default for PrefixFormats {
    fn default() -> Self {
        PrefixFormats {
            agenda: " %i %-12:c%?-12t% s".to_string(),
            todo: " %i %-12:c".to_string(),
            tags: " %i %-12:c".to_string(),
            search: " %i %-12:c".to_string(),
        }
    }
}

/// `org-agenda-tags-column`: where tags end (negative) or start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagsColumn {
    Column(i64),
    /// At the right edge of the window, as Emacs's `auto`; at column 80
    /// when the width is not known.
    Auto,
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
    /// `org-agenda-tags-column`
    pub tags_column: TagsColumn,
    /// The width of the window the agenda is shown in, in columns, when
    /// the editor says: where `TagsColumn::Auto` aligns tags.
    pub window_width: Option<usize>,
    /// `org-agenda-use-time-grid`
    pub use_time_grid: bool,
    /// `org-agenda-time-grid` and `org-agenda-current-time-string`
    pub time_grid: TimeGrid,
    /// `org-agenda-show-current-time-in-grid`
    pub show_current_time_in_grid: bool,
    /// `org-agenda-weekend-days`
    pub weekend_days: Vec<usize>,
    /// `org-priority-highest`, `-lowest` and `-default`, for files that
    /// do not set their own with `#+PRIORITIES:`.
    pub priorities: Priorities,
    /// `org-agenda-show-log`: log mode, `l`.
    pub show_log: bool,
    /// `org-agenda-log-mode-items`
    pub log_items: LogItems,
    /// `org-agenda-clockreport-mode`: the clock report, `R`.
    pub clockreport: bool,
    /// `org-agenda-block-separator`: the lines between this block and
    /// the one before it, in a block agenda; none with `None`.
    pub block_separator: Option<String>,
    /// `org-agenda-compact-blocks`: no separators between blocks, and no
    /// header over agenda blocks.
    pub compact_blocks: bool,
    /// Leave out a block of a block agenda that shows no entries. Not in
    /// Org.
    pub hide_empty_blocks: bool,
    /// `org-agenda-archives-mode`: entries in archived trees are shown
    /// (`v a`). Archive files are the caller's to add (`v A`).
    pub archives: bool,
    /// `org-agenda-prefix-format`
    pub prefix_format: PrefixFormats,
    /// How the search view reads its search string.
    pub search: SearchSettings,
    /// `org-agenda-todo-keyword-format`
    pub todo_keyword_format: String,
    /// `org-agenda-scheduled-leaders`: on its day, and still open after it
    /// (`%d` the days since).
    pub scheduled_leaders: [String; 2],
    /// `org-agenda-deadline-leaders`: on its day, before it (`%d` the days
    /// left) and after it (the days since).
    pub deadline_leaders: [String; 3],
    /// `org-agenda-timerange-leaders`: a range within a day, and one over
    /// days (`%d` the day of the range, `%d` its length).
    pub timerange_leaders: [String; 2],
    /// `org-agenda-remove-tags`
    pub remove_tags: RemoveTags,
    /// `org-agenda-breadcrumbs-separator`
    pub breadcrumbs_separator: String,
    /// `org-agenda-format-date` as a `format-time-string` format; `None`
    /// for `org-agenda-format-date-aligned`.
    pub format_date: Option<String>,
    /// `org-stuck-projects`
    pub stuck_projects: StuckProjects,
    /// `org-tag-alist` and `org-tag-persistent-alist`: the group tags of
    /// files without `#+TAGS:` lines, and of every file.
    pub tag_alist: Vec<crate::tags::TagEntry>,
    pub tag_persistent_alist: Vec<crate::tags::TagEntry>,
    /// `org-group-tags`: a group tag in a match string stands for its tags.
    pub group_tags: bool,
}

/// `org-tag-groups-alist-for-agenda`: the group tags of the files, those
/// of the same name merged.
pub fn tag_groups(files: &[&Outline], config: &Config) -> Vec<(String, Vec<String>)> {
    let settings = crate::tags::Settings {
        alist: config.tag_alist.clone(),
        persistent: config.tag_persistent_alist.clone(),
        ..crate::tags::Settings::default()
    };
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for outline in files {
        let alist = crate::tags::current_alist(&outline.text, &settings);
        for (key, members) in crate::tags::alist_to_groups(&alist) {
            match groups.iter_mut().find(|(known, _)| *known == key) {
                Some((_, old)) => {
                    for member in members {
                        if !old.contains(&member) {
                            old.push(member);
                        }
                    }
                }
                None => groups.insert(0, (key, members)),
            }
        }
    }
    groups
}

/// The matcher with the group tags of the files expanded.
fn expanded(matcher: &Matcher, files: &[&Outline], config: &Config) -> Matcher {
    if !config.group_tags {
        return matcher.clone();
    }
    let groups = tag_groups(files, config);
    if groups.is_empty() {
        return matcher.clone();
    }
    let mut expanded = Matcher::new(&matcher::expand_groups(&matcher.text, &groups));
    expanded.text = matcher.text.clone();
    expanded
}

/// `org-stuck-projects`: what a project is, and what makes it not stuck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StuckProjects {
    /// The match string that finds projects.
    pub matcher: String,
    /// The TODO keywords of entries that are next actions (`*` for any
    /// not done).
    pub todo: Vec<String>,
    /// The tags of entries that are next actions (`*` for any).
    pub tags: Vec<String>,
    /// An Emacs regexp whose match in a project makes it not stuck.
    pub regexp: String,
}

impl Default for StuckProjects {
    fn default() -> Self {
        StuckProjects {
            matcher: "+LEVEL=2/-DONE".to_string(),
            todo: ["TODO", "NEXT", "NEXTACTION"].iter().map(|word| word.to_string()).collect(),
            tags: Vec::new(),
            regexp: String::new(),
        }
    }
}

/// What log mode shows (`org-agenda-log-mode-items`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogItems {
    pub closed: bool,
    pub clock: bool,
    pub state: bool,
}

impl Default for LogItems {
    fn default() -> Self {
        LogItems { closed: true, clock: true, state: false }
    }
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
            tags_column: TagsColumn::Auto,
            window_width: None,
            use_time_grid: true,
            time_grid: TimeGrid::default(),
            show_current_time_in_grid: true,
            weekend_days: vec![6, 0],
            priorities: Priorities::default(),
            show_log: false,
            log_items: LogItems::default(),
            clockreport: false,
            block_separator: Some(default_separator()),
            compact_blocks: false,
            hide_empty_blocks: false,
            archives: false,
            prefix_format: PrefixFormats::default(),
            search: SearchSettings::default(),
            todo_keyword_format: "%-1s".to_string(),
            scheduled_leaders: ["Scheduled: ".to_string(), "Sched.%2dx: ".to_string()],
            deadline_leaders: ["Deadline:  ".to_string(), "In %3d d.: ".to_string(), "%2d d. ago: ".to_string()],
            timerange_leaders: [String::new(), "(%d/%d): ".to_string()],
            remove_tags: RemoveTags::No,
            breadcrumbs_separator: "->".to_string(),
            stuck_projects: StuckProjects::default(),
            tag_alist: Vec::new(),
            tag_persistent_alist: Vec::new(),
            group_tags: true,
            format_date: None,
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
    /// The view's `org-agenda-prefix-format`, compiled.
    format: PrefixFormat,
}

/// Which of `org-agenda-prefix-format`'s formats a view uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewKind {
    Agenda,
    Todo,
    Tags,
    Search,
}

impl<'a> Builder<'a> {
    fn new(files: &'a [&'a Outline], config: &'a Config, today: Day, kind: ViewKind) -> Builder<'a> {
        let formats = &config.prefix_format;
        let format = match kind {
            ViewKind::Agenda => &formats.agenda,
            ViewKind::Todo => &formats.todo,
            ViewKind::Tags => &formats.tags,
            ViewKind::Search => &formats.search,
        };
        Builder { files, config, today, format: PrefixFormat::new(format) }
    }

    /// `%b`: the titles above the entry, each followed by the separator
    /// (`org-format-outline-path` in a window of 80 columns).
    fn breadcrumbs(&self, file: usize, entry: usize) -> String {
        if !self.format.has_breadcrumbs {
            return String::new();
        }
        let outline = self.files[file];
        let mut path = Vec::new();
        let mut parent = outline.entries[entry].parent;
        while let Some(index) = parent {
            let above = &outline.entries[index];
            path.push(crate::refile::heading_path_part(above.level, &above.head, &outline.todo));
            parent = above.parent;
        }
        path.reverse();
        let separator = &self.config.breadcrumbs_separator;
        let mut joined = path.iter().map(|part| part.trim_end_matches([' ', '\t'])).collect::<Vec<_>>().join(separator);
        let width = 79;
        if joined.chars().count() > width {
            joined = joined.chars().take(width - 2).collect::<String>() + "..";
        }
        if joined.is_empty() {
            joined
        } else {
            joined + separator
        }
    }
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
        let breadcrumbs = self.breadcrumbs(file, entry_index);
        let formatted = format_item(&FormatInput {
            format: &self.format,
            extra,
            head: &entry.head,
            category: &entry.category,
            tags: &entry.tags,
            dotime,
            remove_stamps: kind == ItemKind::Timestamp,
            time_trailer: &self.config.time_grid.trailer,
            level: entry.level,
            effort: effort(entry),
            breadcrumbs: &breadcrumbs,
            remove_tags: self.config.remove_tags,
        });
        let mut text = formatted.text;
        let priority = priority(&text, outline.priorities.as_ref().unwrap_or(&self.config.priorities));
        let keyword_len =
            normalise_keyword(&mut text, formatted.heading, outline.todo.names(), &self.config.todo_keyword_format)
                .unwrap_or(0);
        let keyword_done = outline.todo.is_done(&text[formatted.heading..formatted.heading + keyword_len]);
        Item {
            text,
            heading: formatted.heading,
            time_of_day: formatted.time_of_day,
            priority,
            kind,
            face,
            source: Some(Source { file, entry: entry_index }),
            marker: None,
            keyword_len,
            keyword_done,
        }
    }

    /// `org-agenda-get-deadlines`
    fn deadlines(&self, file: usize, current: Day, out: &mut Vec<Item>) {
        let is_today = current == self.today;
        for (index, entry) in self.files[file].entries.iter().enumerate() {
            let found = entry.deadline.as_ref().filter(|planning| planning.active && planning.searchable);
            let Some(planning) = found.filter(|_| !entry.skipped(self.config.archives)) else {
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
            let [now, future, past] = &self.config.deadline_leaders;
            let extra = if is_today && deadline < self.today {
                format_numbers(past, &[-diff])
            } else if is_today && deadline > self.today {
                format_numbers(future, &[diff])
            } else {
                now.clone()
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
            item.marker = Some(planning.offset);
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
            let Some(planning) = found.filter(|_| !entry.skipped(self.config.archives)) else {
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
            let [first, later] = &self.config.scheduled_leaders;
            let extra = if is_today && past { format_numbers(later, &[diff]) } else { first.clone() };
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
            item.marker = Some(planning.offset);
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
            let leader = &self.config.timerange_leaders[usize::from(first != last)];
            let extra = format_numbers(leader, &[1 + current - first, 1 + last - first]);
            let face = if first == last { Face::CalendarEvent } else { Face::CalendarDateRange };
            // Times count on the first and on the last day.
            let dotime = match (first == current, last == current) {
                (true, true) => Some(format!("<{}>--<{}>", block.first, block.second)),
                (true, false) => Some(format!("<{}>", block.first)),
                (false, true) => Some(format!("<{}>", block.second)),
                (false, false) => None,
            };
            let dotime = dotime.as_deref().map_or(Dotime::No, Dotime::Text);
            let mut item = self.item(file, block.entry, ItemKind::Block, face, &extra, dotime);
            item.marker = Some(block.offset);
            out.push(item);
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
            let mut item =
                self.item(file, shown.entry, ItemKind::Timestamp, Face::CalendarEvent, "", Dotime::Text(&shown.text));
            item.marker = Some(stamp.offset);
            out.push(item);
        }
    }

    /// `org-agenda-get-progress`: what log mode shows on `current`.
    fn progress(&self, file: usize, current: Day, out: &mut Vec<Item>) {
        let items = self.config.log_items;
        let outline = self.files[file];
        for log in &outline.logs {
            let wanted = match log.kind {
                LogKind::Closed => items.closed,
                LogKind::Clock(_) => items.clock,
                LogKind::State(_) => items.state,
            };
            if !wanted || log.date.to_day() != current || outline.entries[log.entry].skipped(self.config.archives) {
                continue;
            }
            let extra = match &log.kind {
                LogKind::Closed => "Closed:    ".to_string(),
                LogKind::State(state) => format!("State:     ({state})"),
                LogKind::Clock(clocked) => format!("Clocked:   ({clocked})"),
            };
            let entry = &outline.entries[log.entry];
            let breadcrumbs = self.breadcrumbs(file, log.entry);
            // The note goes before the tags.
            let head = match &log.note {
                None => entry.head.clone(),
                Some(note) => match crate::text::tag_group(&entry.head) {
                    Some((blanks, tags)) => {
                        format!("{} - {note} {}", &entry.head[..blanks], &entry.head[tags..])
                    }
                    None => format!("{} - {note}", entry.head),
                },
            };
            let formatted = format_item(&FormatInput {
                format: &self.format,
                extra: &extra,
                head: &head,
                category: &entry.category,
                tags: &entry.tags,
                dotime: Dotime::Text(&log.time),
                remove_stamps: false,
                time_trailer: &self.config.time_grid.trailer,
                level: entry.level,
                effort: effort(entry),
                breadcrumbs: &breadcrumbs,
                remove_tags: self.config.remove_tags,
            });
            let mut text = formatted.text;
            let keyword_len =
                normalise_keyword(&mut text, formatted.heading, outline.todo.names(), &self.config.todo_keyword_format)
                    .unwrap_or(0);
            let keyword_done = outline.todo.is_done(&text[formatted.heading..formatted.heading + keyword_len]);
            out.push(Item {
                text,
                heading: formatted.heading,
                time_of_day: formatted.time_of_day,
                priority: 100_000,
                kind: ItemKind::Log,
                face: Face::Done,
                source: Some(Source { file, entry: log.entry }),
                marker: Some(log.offset),
                keyword_len,
                keyword_done,
            });
        }
    }

    /// `org-agenda-get-day-entries` for one file, in Org's order.
    fn day(&self, file: usize, current: Day, out: &mut Vec<Item>) {
        self.deadlines(file, current, out);
        if self.config.show_log {
            self.progress(file, current, out);
        }
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
                format: &self.format,
                extra: "",
                head: text,
                category: "",
                tags: &[],
                dotime: Dotime::Text(time),
                remove_stamps: false,
                time_trailer: &grid.trailer,
                level: 0,
                effort: None,
                breadcrumbs: "",
                remove_tags: self.config.remove_tags,
            });
            Item {
                text: formatted.text,
                heading: formatted.heading,
                time_of_day: formatted.time_of_day,
                priority: 0,
                kind: ItemKind::Grid,
                face,
                source: None,
                marker: None,
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
    let builder = Builder::new(files, config, today, ViewKind::Agenda);
    let mut items = Vec::new();
    for file in 0..files.len() {
        builder.day(file, current, &mut items);
    }
    sort(&mut items, true);
    items
}

/// `org-agenda-get-todos` over all files: every entry in a not-done state,
/// or with one of `keywords` (`org-todo-list` with an argument).
pub fn todo_items(files: &[&Outline], keywords: Option<&[String]>, config: &Config) -> Vec<Item> {
    let builder = Builder::new(files, config, 0, ViewKind::Todo);
    let mut items = Vec::new();
    for (file, outline) in files.iter().enumerate() {
        for (index, entry) in outline.entries.iter().enumerate() {
            let wanted = match keywords {
                Some(keywords) => entry.todo.as_ref().is_some_and(|todo| keywords.contains(todo)),
                None => entry.is_open_todo(),
            };
            if entry.skipped(config.archives) || !wanted {
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
    /// Between the blocks of a block agenda.
    Separator,
    /// A line of the clock report.
    Table,
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
    /// The day after the last one shown.
    pub end: Day,
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
    let column = match config.tags_column {
        TagsColumn::Column(column) => column,
        TagsColumn::Auto => -(config.window_width.unwrap_or(80) as i64),
    };
    Line { text: align_tags(&item.text, column), kind: LineKind::Item, item: Some(item) }
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

/// `org-agenda-compute-starting-span`: the first day of the `span` that
/// contains `day`, used when the span of an open agenda changes.
pub fn starting_day(day: Day, span: Span, config: &Config) -> Day {
    let date = Date::from_day(day);
    match span {
        Span::Day | Span::Days(_) => day,
        Span::Week | Span::Fortnight => match config.start_on_weekday {
            Some(first) => day - (weekday(day) as i64 - first as i64).rem_euclid(7),
            None => day,
        },
        Span::Month => Date::new(date.year, date.month, 1).to_day(),
        Span::Year => Date::new(date.year, 1, 1).to_day(),
    }
}

/// `org-agenda-format-date`: a `format-time-string` format, or
/// `org-agenda-format-date-aligned`.
fn date_heading(day: Day, config: &Config) -> String {
    if let Some(format) = &config.format_date {
        return crate::capture::format_time(format, crate::date::DateTime { day, minute: 0 });
    }
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
    let builder = Builder::new(files, config, now.today, ViewKind::Agenda);
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
    let mut lines = Vec::new();
    if !config.compact_blocks {
        lines.push(Line { text: format!("{}-agenda{weeks}:", span.name()), kind: LineKind::Header, item: None });
    }

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
            text: date_heading(day, config),
            kind: LineKind::Date { day, today: is_today, weekend: config.weekend_days.contains(&weekday(day)) },
            item: None,
        });
        builder.add_time_grid(&mut items, ndays, is_today, now.minute);
        sort(&mut items, true);
        lines.extend(items.into_iter().map(|item| item_line(item, config)));
    }
    View { lines, start, end: last + 1 }
}

/// `org-todo-list`: the global list of open TODO entries, or of the
/// entries with one of the keywords of `keywords` (`NEXT|WAIT`).
pub fn todo_view(files: &[&Outline], keywords: Option<&str>, config: &Config) -> View {
    let wanted: Option<Vec<String>> =
        keywords.filter(|text| !text.trim().is_empty()).map(|text| text.split('|').map(str::to_string).collect());
    let shown = wanted.as_ref().map_or("ALL".to_string(), |keywords| keywords.join("|"));
    let header = format!("Global list of TODO items of type: {shown}");
    let mut lines = vec![Line { text: header, kind: LineKind::Header, item: None }];
    lines.extend(todo_items(files, wanted.as_deref(), config).into_iter().map(|item| item_line(item, config)));
    View { lines, start: 0, end: 0 }
}

/// `org-tags-view`: the entries `matcher` matches, only open TODO
/// entries with `todo_only` (`tags-todo`).
pub fn tags_view(files: &[&Outline], matcher: &Matcher, todo_only: bool, now: Now, config: &Config) -> View {
    let matcher = &expanded(matcher, files, config);
    let builder = Builder::new(files, config, now.today, ViewKind::Tags);
    let clock = matcher::Clock { today: now.today, minute: now.minute };
    let mut items = Vec::new();
    for (file, outline) in files.iter().enumerate() {
        for index in 0..outline.entries.len() {
            if outline.entries[index].skipped(config.archives) || !matcher.matches(outline, index, todo_only, clock) {
                continue;
            }
            items.push(builder.item(file, index, ItemKind::Todo, Face::Default, "", Dotime::Headline));
        }
    }
    sort(&mut items, false);
    let header = format!("Headlines with TAGS match: {}", matcher.text);
    let mut lines = vec![Line { text: header, kind: LineKind::Header, item: None }];
    lines.extend(items.into_iter().map(|item| item_line(item, config)));
    View { lines, start: 0, end: 0 }
}

/// `org-agenda-list-stuck-projects` (`#`): the projects (the entries the
/// match string of `org-stuck-projects` finds) with no next action in
/// their subtree: no entry there with one of its TODO keywords or tags,
/// and no match of its regexp; the subtree of a project that is not stuck
/// is still searched for projects.
pub fn stuck_view(files: &[&Outline], now: Now, config: &Config) -> Result<View, String> {
    let stuck = &config.stuck_projects;
    let matcher = expanded(&Matcher::new(&stuck.matcher), files, config);
    let todo_words: Vec<String> = if stuck.todo.iter().any(|word| word == "*") {
        let mut words: Vec<String> = Vec::new();
        for outline in files {
            for word in outline.todo.names() {
                if !outline.todo.is_done(word) && !words.iter().any(|known| known == word) {
                    words.push(word.to_string());
                }
            }
        }
        words
    } else {
        stuck.todo.clone()
    };
    let mut patterns: Vec<String> = Vec::new();
    if !stuck.todo.is_empty() {
        let words: Vec<String> = todo_words.iter().map(|word| regex::escape(word)).collect();
        patterns.push(format!(r"^\*+[ \t]+({})(?:[ \t]|$)", words.join("|")));
    }
    if stuck.tags.iter().any(|tag| tag == "*") {
        patterns.push(r"^\*+ (?:.*[ \t])?(:([\w@#%:]+):)[ \t]*$".to_string());
    } else if !stuck.tags.is_empty() {
        let other = r"(?:[\w@#%]+:)*";
        let tags: Vec<String> = stuck.tags.iter().map(|tag| regex::escape(tag)).collect();
        patterns.push(format!(r"^\*+ .*?[ \t]:{other}({}):{other}[ \t]*$", tags.join("|")));
    }
    if !stuck.regexp.trim().is_empty() {
        patterns.push(matcher::translate(&stuck.regexp));
    }
    if patterns.is_empty() {
        return Err("Missing information to identify unstuck projects".to_string());
    }
    let skip: Vec<regex::Regex> = patterns
        .iter()
        .map(|pattern| regex::RegexBuilder::new(pattern).multi_line(true).build())
        .collect::<Result<_, _>>()
        .map_err(|error| format!("Invalid regexp in org-stuck-projects: {error}"))?;
    let builder = Builder::new(files, config, now.today, ViewKind::Tags);
    let clock = matcher::Clock { today: now.today, minute: now.minute };
    let mut items = Vec::new();
    for (file, outline) in files.iter().enumerate() {
        for index in 0..outline.entries.len() {
            let entry = &outline.entries[index];
            if entry.skipped(config.archives) || !matcher.matches(outline, index, false, clock) {
                continue;
            }
            // `org-end-of-subtree`: before the next headline as high, and
            // before a blank line there.
            let next = outline.entries[index + 1..]
                .iter()
                .find(|other| other.level <= entry.level)
                .map_or(outline.text.len(), |other| other.offset);
            let mut end = next;
            for _ in 0..2 {
                if outline.text[..end].ends_with('\n') && end > entry.offset {
                    end -= 1;
                }
            }
            let subtree = &outline.text[entry.offset..end];
            if skip.iter().any(|pattern| pattern.is_match(subtree)) {
                continue;
            }
            items.push(builder.item(file, index, ItemKind::Todo, Face::Default, "", Dotime::Headline));
        }
    }
    sort(&mut items, false);
    let mut lines = vec![Line { text: "List of stuck projects: ".to_string(), kind: LineKind::Header, item: None }];
    lines.extend(items.into_iter().map(|item| item_line(item, config)));
    Ok(View { lines, start: 0, end: 0 })
}

/// `org-search-view`: the entries with the words, phrases and regexps of
/// `string` (see [`search::parse`]), in file order; only open TODO
/// entries with `todo_only` (`S`).
pub fn search_view(
    files: &[&Outline],
    string: &str,
    todo_only: bool,
    now: Now,
    config: &Config,
) -> Result<View, String> {
    let search = search::parse(string, todo_only, &config.search)?;
    let builder = Builder::new(files, config, now.today, ViewKind::Search);
    let max = config.search.max_level;
    let deep = |outline: &Outline, entry: usize| max != 0 && outline.entries[entry].level > max;
    let mut items = Vec::new();
    for (file, outline) in files.iter().enumerate() {
        let text = outline.text.as_str();
        let Some(first) = outline.entries.first() else { continue };
        // `org-not-done-regexp` of the file, after the stars.
        let open: Vec<String> =
            outline.todo.names().filter(|name| !outline.todo.is_done(name)).map(regex::escape).collect();
        let todo = regex::RegexBuilder::new(&format!(r"\A\*+[ \t]+\b(?:{})\b", open.join("|")))
            .case_insensitive(true)
            .build()
            .map_err(|error| error.to_string())?;
        let mut at = floor_char(text, first.offset.saturating_sub(1));
        while let Some((_, last_end)) = search.regexp.find_at(text, at) {
            // `org-back-to-heading`, then up to an entry the view shows.
            let Some(mut entry) = outline.entries.iter().rposition(|entry| entry.offset <= last_end) else { break };
            while deep(outline, entry) && entry > 0 {
                entry -= 1;
            }
            let begin = outline.entries[entry].offset;
            let mut next = entry + 1;
            while next < outline.entries.len() && deep(outline, next) {
                next += 1;
            }
            let end = outline.entries.get(next).map_or(text.len(), |entry| entry.offset);
            let before_end = floor_char(text, end.saturating_sub(1));
            let line_end = text[begin..].find('\n').map_or(text.len(), |at| begin + at);
            let seen = &text[begin..if search.headline_only { line_end } else { end }];
            let wanted = !outline.entries[entry].skipped(config.archives)
                && !search.minus.iter().any(|pattern| pattern.is_match(seen))
                && (!search.todo_only || todo.is_match(seen))
                && search.plus.iter().all(|pattern| pattern.is_match(seen));
            let resume = if wanted {
                items.push(builder.item(file, entry, ItemKind::Todo, Face::Default, "", Dotime::Headline));
                before_end.max(last_end)
            } else {
                before_end
            };
            // Always on: an empty match at the end of an entry.
            at = if resume > at { resume } else { next_char(text, at) };
            if at >= text.len() {
                break;
            }
        }
    }
    // Every line has the same priority (1000), and `category-keep` keeps
    // the order: as found.
    let header = format!("Search words: {string}");
    let mut lines = vec![Line { text: header, kind: LineKind::Header, item: None }];
    lines.extend(items.into_iter().map(|item| item_line(item, config)));
    Ok(View { lines, start: 0, end: 0 })
}

fn floor_char(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn next_char(text: &str, at: usize) -> usize {
    text[at..].chars().next().map_or(text.len(), |c| at + c.len_utf8())
}

/// `org-agenda-overriding-header`: another header for a view, or none
/// when it is empty.
pub fn override_header(view: &mut View, header: &str) {
    if view.lines.first().is_some_and(|line| line.kind == LineKind::Header) {
        view.lines.remove(0);
    }
    if !header.is_empty() {
        view.lines.insert(0, Line { text: header.to_string(), kind: LineKind::Header, item: None });
    }
}

/// The default `org-agenda-block-separator`: `=` across the window,
/// which in Emacs's batch mode is 79 columns.
pub fn default_separator() -> String {
    "=".repeat(79)
}

/// The lines between a block and the one before it in a block agenda, as
/// its `org-agenda-block-separator` and `org-agenda-compact-blocks` have
/// them: a blank line and the separator.
pub fn block_separator(config: &Config) -> Vec<Line> {
    let Some(separator) = config.block_separator.as_ref().filter(|_| !config.compact_blocks) else {
        return Vec::new();
    };
    let line = |text: &str| Line { text: text.to_string(), kind: LineKind::Separator, item: None };
    std::iter::once(line("")).chain(separator.split('\n').map(line)).collect()
}

/// `%e`: the entry's effort (`org-effort-property`).
fn effort(entry: &Entry) -> Option<&str> {
    entry.properties.iter().find(|(name, _)| name == "EFFORT").map(|(_, value)| value.as_str())
}

/// The entry an agenda line was made from.
pub fn entry<'a>(files: &[&'a Outline], source: Source) -> &'a Entry {
    &files[source.file].entries[source.entry]
}
