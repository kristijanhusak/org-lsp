//! The agenda as data: each line with the text properties Org's
//! `org-batch-agenda-csv` prints (`org-fix-agenda-info`), for
//! `org-lsp agenda --format csv` (as Emacs prints them) and
//! `--format json` (grouped by block and day).

use org_core::agenda::{self, ItemKind, LineKind, Now, Span};
use org_core::date::{Date, Day};
use org_core::outline::Outline;
use serde_json::{json, Value};

use crate::views::{self, Block, BlockKind, SearchFiles};

/// One line of a view as `org-fix-agenda-info` makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// `org-category`: the entry's whole category, the file name for a
    /// tags match without one; empty for the time grid.
    pub category: String,
    /// `txt`: the headline without TODO keyword, priority cookie and tags.
    pub head: String,
    /// `type`: `deadline`, `upcoming-deadline`, `scheduled`,
    /// `past-scheduled`, `timestamp`, `block`, `sexp`, `todo…`,
    /// `tagsmatch…`, `search`, `closed`, `clock` or `state`; empty for the
    /// time grid.
    pub kind: String,
    pub todo: Option<String>,
    pub tags: Vec<String>,
    /// `date`: the day the line is about.
    pub date: Option<Day>,
    /// `time`, such as `9:30-9:45` or `8:00......`.
    pub time: String,
    /// `extra`: the leader, such as `Scheduled:` or `In   2 d.:`.
    pub extra: String,
    pub priority_letter: Option<String>,
    /// `priority`: the headline's (`org-get-priority`); none for the grid.
    pub priority: Option<i64>,
    /// `urgency`: the priority as the agenda sorts by it.
    pub urgency: Option<i64>,
    /// `agenda-day`: the day the line is under, in an agenda.
    pub agenda_day: Option<Day>,
    /// The line as printed.
    pub text: String,
    /// The entry: the index of its file and the byte offset of its headline.
    pub entry: Option<(usize, usize)>,
    /// A line of the time grid (`org-agenda-time-grid`), current time too.
    pub grid: bool,
    /// The org-super-agenda group the line is in.
    pub group: Option<String>,
    /// The entry's text under the line (`org-agenda-entry-text-mode`).
    pub entry_text: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDay {
    pub day: Day,
    pub today: bool,
    pub weekend: bool,
    pub heading: String,
    pub records: Vec<Record>,
}

/// A block of the view: an agenda's lines are under its days, the
/// others' in `records`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordBlock {
    /// `org-agenda-type`
    pub kind: &'static str,
    /// The header lines above the entries.
    pub header: Vec<String>,
    /// An agenda's first day and span.
    pub agenda: Option<(Day, Span)>,
    pub days: Vec<RecordDay>,
    pub records: Vec<Record>,
    /// The clock report under an agenda, line by line.
    pub clockreport: Vec<String>,
}

impl RecordBlock {
    /// The records in the order of the view.
    pub fn all(&self) -> impl Iterator<Item = &Record> {
        self.records.iter().chain(self.days.iter().flat_map(|day| day.records.iter()))
    }
}

/// The view `blocks` make, as records.
pub fn records(
    blocks: &[Block],
    files: &[(&Outline, String, String)],
    search: Option<&SearchFiles>,
    now: Now,
    filters: Option<&agenda::filter::Filters>,
) -> Vec<RecordBlock> {
    let outlines: Vec<&Outline> = files.iter().map(|(outline, _, _)| *outline).collect();
    let (lines, starts) = views::block_lines(blocks, files, search, now, true, filters);
    let mut out: Vec<(usize, RecordBlock)> = Vec::new();
    let mut group: Option<String> = None;
    for (number, line) in lines {
        if out.last().is_none_or(|(last, _)| *last != number) {
            let block = &blocks[number];
            out.push((
                number,
                RecordBlock {
                    kind: block.kind.agenda_type(),
                    header: Vec::new(),
                    agenda: starts.get(number).copied().flatten(),
                    days: Vec::new(),
                    records: Vec::new(),
                    clockreport: Vec::new(),
                },
            ));
            group = None;
        }
        let (number, block) = out.last_mut().expect("a block was pushed");
        let config = &blocks[*number].config;
        let agenda = matches!(blocks[*number].kind, BlockKind::Agenda { .. });
        match line.kind {
            LineKind::Header if block.days.is_empty() && block.records.is_empty() && block.clockreport.is_empty() => {
                block.header.push(line.text)
            }
            LineKind::Header | LineKind::Table => block.clockreport.push(line.text),
            LineKind::Date { day, today, weekend } => {
                group = None;
                block.days.push(RecordDay { day, today, weekend, heading: line.text, records: Vec::new() });
            }
            LineKind::Group { .. } => group = Some(line.text.trim().to_string()),
            LineKind::EntryText => {
                let last = block.days.last_mut().map_or(&mut block.records, |day| &mut day.records);
                if let Some(record) = last.last_mut() {
                    record.entry_text.push(line.text);
                }
            }
            LineKind::Item => {
                let Some(item) = &line.item else { continue };
                let day = block.days.last().filter(|_| agenda).map(|day| day.day);
                let today = now.org_today(config.extend_today_until);
                let mut record = record(item, &outlines, day, today, line.text);
                record.group = group.clone();
                match block.days.last_mut().filter(|_| agenda) {
                    Some(day) => day.records.push(record),
                    None => block.records.push(record),
                }
            }
            LineKind::Separator | LineKind::Blank => {}
        }
    }
    out.into_iter().map(|(_, block)| block).collect()
}

/// The record of the line `text` of `item`, under the agenda's day `day`.
fn record(item: &agenda::Item, outlines: &[&Outline], day: Option<Day>, today: Day, text: String) -> Record {
    let entry = item.source.map(|source| (outlines[source.file], agenda::entry(outlines, source)));
    let grid = item.kind == ItemKind::Grid;
    let category = match entry {
        Some((outline, entry)) if item.kind == ItemKind::Tags && entry.no_category => outline.name.clone(),
        Some((_, entry)) => entry.category.clone(),
        None => String::new(),
    };
    let tags = match entry {
        Some((_, entry)) => {
            entry.tags.iter().filter(|tag| !item.own_tags_only || !tag.inherited).map(|tag| tag.name.clone()).collect()
        }
        None => Vec::new(),
    };
    // `org-agenda-get-deadlines` and `-scheduled`: the date of the
    // deadline unless it is upcoming, of the scheduled date when past.
    let (kind, date) = match item.kind {
        ItemKind::Deadline if day == Some(today) && item.ts_date.is_some_and(|date| date > today) => {
            ("upcoming-deadline".to_string(), day)
        }
        ItemKind::Deadline => ("deadline".to_string(), item.ts_date.or(day)),
        ItemKind::Scheduled if item.ts_date.is_some_and(|date| date < today) => {
            ("past-scheduled".to_string(), item.ts_date)
        }
        ItemKind::Scheduled => ("scheduled".to_string(), day),
        ItemKind::Timestamp => ("timestamp".to_string(), day),
        ItemKind::Block => ("block".to_string(), day),
        ItemKind::Sexp => ("sexp".to_string(), day),
        ItemKind::Log => (item.log.unwrap_or("").to_string(), day),
        ItemKind::Todo => (format!("todo{}", item.ts_type), None),
        ItemKind::Tags => (format!("tagsmatch{}", item.ts_type), None),
        ItemKind::Search => ("search".to_string(), None),
        ItemKind::Grid => (String::new(), None),
    };
    // `org-fix-agenda-info`: the priority cookie and the keyword out of
    // the headline, here also its tags (`org-agenda-remove-tags`).
    let mut head = item.text.get(item.heading..).unwrap_or("").to_string();
    if let Some((blanks, _)) = org_core::text::tag_group(&head) {
        head.truncate(blanks);
    }
    let priority_letter = cookie(&head).map(|(start, end, letter)| {
        head.replace_range(start..end, "");
        letter
    });
    let todo = entry.and_then(|(outline, _)| {
        let dots = head.bytes().take_while(|byte| *byte == b'.').count();
        let keyword = outline
            .todo
            .names()
            .filter(|name| head[dots..].starts_with(name))
            .max_by_key(|name| name.len())?
            .to_string();
        let mut end = dots + keyword.len();
        if head[end..].starts_with(' ') {
            end += 1;
        }
        head.replace_range(..end, "");
        Some(keyword)
    });
    Record {
        category,
        head: head.trim().to_string(),
        kind,
        todo,
        tags,
        date,
        time: item.time.trim().to_string(),
        extra: item.extra.trim().to_string(),
        priority_letter,
        priority: (!grid).then_some(item.base_priority),
        urgency: (!grid).then_some(item.priority),
        agenda_day: day,
        text,
        entry: entry.map(|(_, found)| (item.source.map_or(0, |source| source.file), found.offset)),
        grid,
        group: None,
        entry_text: Vec::new(),
    }
}

/// The first priority cookie of `text` (`\[#\([A-Z0-9]\)\] ?`, any case
/// as `case-fold-search` reads it): where it is, with the blank after it,
/// and its letter.
fn cookie(text: &str) -> Option<(usize, usize, String)> {
    let bytes = text.as_bytes();
    text.match_indices("[#").find_map(|(at, _)| match bytes.get(at + 2..at + 4) {
        Some([letter, b']']) if letter.is_ascii_alphanumeric() => {
            let end = at + 4 + usize::from(bytes.get(at + 4) == Some(&b' '));
            Some((at, end, (*letter as char).to_string()))
        }
        _ => None,
    })
}

/// A day as `org-fix-agenda-info` writes it: `2026-11-25`.
pub fn date_text(day: Day) -> String {
    let date = Date::from_day(day);
    format!("{}-{:02}-{:02}", date.year, date.month, date.day)
}

/// The records as `org-batch-agenda-csv` prints them: a line each, the
/// values without commas, trimmed.
pub fn csv(blocks: &[RecordBlock]) -> String {
    let value = |text: &str| text.replace(',', ";").trim().to_string();
    let date = |day: Option<Day>| day.map(date_text).unwrap_or_default();
    let mut out = String::new();
    for record in blocks.iter().flat_map(RecordBlock::all) {
        let fields = [
            value(&record.category),
            value(&record.head),
            value(&record.kind),
            value(record.todo.as_deref().unwrap_or("")),
            value(&record.tags.join(":")),
            date(record.date),
            value(&record.time),
            value(&record.extra),
            record.priority_letter.clone().unwrap_or_default(),
            record.priority.map(|priority| priority.to_string()).unwrap_or_default(),
            date(record.agenda_day),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out
}

/// The records as JSON, grouped by block and day, without the time grid;
/// `place` gives the file and line (from 1) of an entry.
pub fn json(blocks: &[RecordBlock], today: Day, place: &dyn Fn(usize, usize) -> (String, usize)) -> Value {
    let date = |day: Option<Day>| day.map(date_text);
    let item = |record: &Record| {
        let (file, line) = match record.entry.map(|(file, offset)| place(file, offset)) {
            Some((file, line)) => (Some(file), Some(line)),
            None => (None, None),
        };
        let mut value = json!({
            "category": record.category,
            "head": record.head,
            "type": record.kind,
            "todo": record.todo,
            "tags": record.tags,
            "date": date(record.date),
            "time": (!record.time.is_empty()).then_some(&record.time),
            "extra": record.extra,
            "priority_letter": record.priority_letter,
            "priority": record.priority,
            "urgency": record.urgency,
            "agenda_day": date(record.agenda_day),
            "file": file,
            "line": line,
            "text": record.text,
        });
        if let Some(group) = &record.group {
            value["group"] = json!(group);
        }
        if !record.entry_text.is_empty() {
            value["entry_text"] = json!(record.entry_text);
        }
        value
    };
    let items = |records: &[Record]| -> Vec<Value> { records.iter().filter(|record| !record.grid).map(item).collect() };
    let blocks: Vec<Value> = blocks
        .iter()
        .map(|block| {
            let mut value = json!({
                "type": block.kind,
                "header": block.header.join("\n"),
            });
            match block.agenda {
                Some((start, span)) => {
                    value["span"] = json!(span_name(span));
                    value["start"] = json!(date_text(start));
                    value["end"] = json!(date_text(start + span.days(start) - 1));
                    value["days"] = block
                        .days
                        .iter()
                        .map(|day| {
                            json!({
                                "date": date_text(day.day),
                                "today": day.today,
                                "weekend": day.weekend,
                                "heading": day.heading,
                                "items": items(&day.records),
                            })
                        })
                        .collect();
                }
                None => value["items"] = json!(items(&block.records)),
            }
            if !block.clockreport.is_empty() {
                value["clockreport"] = json!(block.clockreport.join("\n"));
            }
            value
        })
        .collect();
    json!({ "today": date_text(today), "blocks": blocks })
}

/// A span as `--span` takes it.
fn span_name(span: Span) -> Value {
    match span {
        Span::Day => json!("day"),
        Span::Week => json!("week"),
        Span::Fortnight => json!("fortnight"),
        Span::Month => json!("month"),
        Span::Year => json!("year"),
        Span::Days(days) => json!(days),
    }
}
