//! `org-journal--carryover`: the items of the entry before that match
//! `org-journal-carryover-items` move to today's entry, with the
//! headlines above them.

use regex::RegexBuilder;

use super::buffer::search_regex;
use super::{entry_date, format_time, DeleteEmpty, Emacs, Stop};
use crate::agenda::matcher::{Clock, Matcher};
use crate::outline::Outline;

/// An item or a headline above it: where it was, and its text.
#[derive(Debug, Clone)]
pub(super) struct Path {
    start: usize,
    end: usize,
    pub(super) text: String,
}

/// `replace-match` without FIXEDCASE: the case of `replacement` follows
/// `matched`: all capitals when it has no lower case letter and a word of
/// several letters.
fn adjust_case(matched: &str, replacement: &str) -> String {
    let word = |c: char| c.is_alphanumeric();
    let (mut multiletter, mut lowercase, mut uppercase, mut nonuppercase_initial) = (false, false, false, false);
    let mut previous = '\n';
    for c in matched.chars() {
        if c.is_lowercase() {
            lowercase = true;
            if word(previous) {
                multiletter = true
            } else {
                nonuppercase_initial = true
            }
        } else if c.is_uppercase() {
            uppercase = true;
            if word(previous) {
                multiletter = true;
            }
        } else if !word(previous) && word(c) {
            nonuppercase_initial = true;
        }
        previous = c;
    }
    let all_caps = (!lowercase && multiletter) || (!nonuppercase_initial && !multiletter && uppercase);
    if all_caps {
        replacement.to_uppercase()
    } else {
        replacement.to_string()
    }
}

impl Emacs<'_> {
    /// `org-journal--carryover-item-with-parents`, at the item's headline.
    fn item_with_parents(&mut self) -> Result<Vec<Path>, Stop> {
        let level = self
            .settings
            .time_entry_level()
            .ok_or_else(|| Stop::Error("Wrong type argument: number-or-marker-p, nil".to_string()))?;
        let buffer = self.buffer();
        let mut paths = Vec::new();
        buffer.save_point();
        while buffer.outline_level() > level {
            let before = buffer.point;
            buffer.org_up_heading_safe();
            if buffer.point == before {
                // Emacs would loop for ever here.
                break;
            }
            let start = buffer.point;
            buffer.save_point();
            buffer.outline_next_heading();
            let end = buffer.point;
            buffer.restore_point();
            paths.insert(0, Path { start, end, text: buffer.text[start..end].to_string() });
        }
        buffer.restore_point();
        let start = buffer.line_start(buffer.point);
        buffer.outline_end_of_subtree()?;
        buffer.outline_next_heading();
        let end = buffer.point;
        paths.push(Path { start, end, text: buffer.text[start..end].to_string() });
        Ok(paths)
    }

    /// `org-map-entries` with `match` over the accessible portion of the
    /// current buffer, each match's item with its parents, the subtree of
    /// each skipped once taken.
    pub(super) fn items_matching(&mut self, matching: &str) -> Result<Vec<Vec<Path>>, Stop> {
        let buffer = self.buffer().clone();
        let name = buffer.name();
        let name = name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem).to_string();
        let parse = org_syntax::parse(&buffer.text, &self.settings.properties.keywords);
        let outline = Outline::new(&parse, &buffer.text, &name);
        let matcher = Matcher::new(matching);
        let clock = Clock { today: self.now().day, minute: self.now().minute };
        let mut found = Vec::new();
        let mut from = buffer.begv;
        for index in 0..outline.entries.len() {
            let entry = &outline.entries[index];
            if entry.offset < from || entry.offset >= buffer.zv || entry.offset < buffer.begv {
                continue;
            }
            if entry.skipped(false) || !matcher.matches(&outline, index, false, clock) {
                continue;
            }
            let current = self.buffer();
            current.save_point();
            current.point = entry.offset;
            let paths = self.item_with_parents()?;
            let current = self.buffer();
            from = current.point;
            current.restore_point();
            found.push(paths);
        }
        Ok(found)
    }

    /// `org-journal--carryover`
    pub(super) fn carryover(&mut self) -> Result<(), Stop> {
        let here = self.current;
        let restriction = (self.buffer().begv, self.buffer().zv);
        self.buffer().save_point();
        let mut previous = None;
        let mut paths = Vec::new();
        if self.open_entry(true)? {
            previous = self.current;
            if !self.settings.daily() {
                self.buffer().org_narrow_to_subtree()?;
            }
            let matching = self.settings.carryover_items.clone();
            paths = self.items_matching(&matching)?;
        }
        self.current = here;
        // `save-restriction` gives the buffer back its restriction; the
        // buffer before, when another, stays narrowed.
        if previous == here && restriction == (0, self.buffer().text.len()) {
            self.buffer().widen();
        }
        self.buffer().restore_point();
        let Some(previous) = previous.filter(|_| !paths.is_empty()) else { return Ok(()) };
        // The text to carry over, without a headline twice.
        let mut text = String::new();
        let mut cleared: Vec<Path> = Vec::new();
        let mut last: Option<&Vec<Path>> = None;
        for group in &paths {
            for (index, path) in group.iter().enumerate() {
                // As org-journal counts, the path at INDEX is compared
                // with the one after it in the group before.
                let before = last.and_then(|last| last.get(index + 1));
                if before.is_none_or(|before| path.start > before.start) {
                    text.push_str(&path.text);
                    cleared.push(path.clone());
                }
            }
            last = Some(group);
        }
        self.carryover_items(&text, &cleared, previous)?;
        self.delete_empty_journal(previous)
    }

    /// `org-journal-carryover-items`
    fn carryover_items(&mut self, text: &str, entries: &[Path], previous: usize) -> Result<(), Stop> {
        if entries.is_empty() {
            return Ok(());
        }
        let settings = self.settings;
        let heading = settings.date_prefix_is_heading();
        let buffer = self.buffer();
        if heading {
            while buffer.org_up_heading_safe().is_some() {}
            buffer.outline_end_of_subtree()?;
        } else {
            buffer.point = buffer.zv;
        }
        if !buffer.bolp() {
            buffer.insert("\n");
        }
        buffer.insert(text);
        buffer.save_point();
        if settings.daily() {
            buffer.point = buffer.begv;
        } else {
            while buffer.org_up_heading_safe().is_some() {}
        }
        if !settings.skip_carryover_drawers.is_empty() {
            self.remove_drawers()?;
        }
        self.move_timestamps()?;
        let buffer = self.buffer();
        buffer.restore_point();
        buffer.outline_end_of_subtree()?;
        // `org-journal-delete-old-carryover`, in the buffer before.
        let old = &mut self.buffers[previous];
        for entry in entries.iter().rev() {
            old.save_point();
            // A character back from the end, which may be several bytes.
            let back = old.text[..entry.end.min(old.text.len())].char_indices().next_back().map_or(0, |(at, _)| at);
            old.goto(back);
            let has_child = old.org_goto_first_child();
            old.restore_point();
            if !has_child {
                old.delete(entry.start, entry.end);
            }
        }
        Ok(())
    }

    /// `org-journal--remove-drawer`
    fn remove_drawers(&mut self) -> Result<(), Stop> {
        let daily = self.settings.daily();
        let drawers = self.settings.skip_carryover_drawers.clone();
        let buffer = self.buffer();
        buffer.save_point();
        let whole = buffer.begv == 0 && buffer.zv == buffer.text.len();
        if !daily {
            buffer.org_narrow_to_subtree()?;
        }
        buffer.point = buffer.begv;
        for name in drawers {
            // `delete-matching-lines`, from the start: case matters, as the
            // regexp has capitals.
            let pattern = format!(r".*{}:[\x00-\x7F]+?:END:$", regex::escape(&name));
            let regex = RegexBuilder::new(&pattern).multi_line(true).build().expect("valid pattern");
            buffer.save_point();
            while buffer.point < buffer.zv {
                let Some(found) = buffer.re_search_forward(&regex, None, false) else { break };
                let start = buffer.line_start(found.start());
                buffer.forward_line(1);
                let end = buffer.point;
                buffer.delete(start, end);
            }
            buffer.restore_point();
        }
        if whole {
            buffer.widen();
        }
        buffer.restore_point();
        Ok(())
    }

    /// The active timestamps of the carried items, but those of planning
    /// lines, get the date of the new entry.
    fn move_timestamps(&mut self) -> Result<(), Stop> {
        let stamp = search_regex(r"<([0-9]{4}-[0-9]{2}-[0-9]{2}( [a-z]+)?)>");
        let planning = search_regex(r"\b(SCHEDULED|DEADLINE):");
        self.buffer().save_point();
        loop {
            let buffer = self.buffer();
            let Some(found) = buffer.re_search_forward(&stamp, None, false) else { break };
            let start = buffer.line_start(buffer.point);
            let end = buffer.line_end(buffer.point);
            if planning.find(&buffer.text[start..end]).is_some() {
                continue;
            }
            let date = if self.settings.daily() {
                let file = self.buffer().file.clone();
                self.files.date(&file)?
            } else {
                let buffer = self.buffer();
                buffer.save_point();
                while buffer.org_up_heading_safe().is_some() {}
                let snapshot = buffer.clone();
                buffer.restore_point();
                entry_date(self.settings, &snapshot, snapshot.point)?
            };
            let group = found.group(1).expect("the date");
            let buffer = self.buffer();
            let replacement = adjust_case(&buffer.text[group.clone()], &format_time("%Y-%m-%d %a", date.time()));
            buffer.delete(group.start, group.end);
            buffer.point = group.start;
            buffer.insert(&replacement);
        }
        self.buffer().restore_point();
        Ok(())
    }

    /// `org-journal--empty-journal-p`: whether the entry before the
    /// current one has nothing left but planning and drawers.
    fn empty_journal(&mut self, previous: usize) -> Result<bool, Stop> {
        self.save(previous)?;
        let here = self.current;
        self.buffer().save_point();
        self.open_entry(true)?;
        let heading = self.settings.date_prefix_is_heading();
        let entry = {
            let buffer = self.buffer();
            if heading {
                let mut probe = buffer.clone();
                probe.org_back_to_heading(true)?;
                let start =
                    probe.text[probe.point..probe.zv].find('\n').map_or(probe.zv, |newline| probe.point + newline + 1);
                let end = probe.org_end_of_subtree(true, false);
                buffer.text[start.min(end)..start.max(end)].to_string()
            } else {
                buffer.text[buffer.point..buffer.zv].to_string()
            }
        };
        self.current = here;
        self.buffer().restore_point();
        let planning =
            search_regex(r" *(?:CLOSED|DEADLINE|SCHEDULED): *[\[<]([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)[\]>]");
        let mut text = planning.replace_all(&entry, "").into_owned();
        let drawer = search_regex(r"^[ \t]*:((?:\w|[-_])+):[ \t]*$");
        let mut at = 0;
        while let Some(first) = drawer.find_at(&text, at).map(|found| found.range()) {
            let end = drawer.find_at(&text, first.end).map_or(first.end, |second| second.end());
            text.replace_range(first.start..end, "");
            at = first.start;
        }
        Ok(text.trim_matches([' ', '\t', '\n', '\r']).is_empty())
    }

    /// `org-journal--carryover-delete-empty-journal`
    fn delete_empty_journal(&mut self, previous: usize) -> Result<(), Stop> {
        let empty = self.empty_journal(previous)?;
        let delete = empty
            && match self.settings.carryover_delete_empty_journal {
                DeleteEmpty::Never => false,
                DeleteEmpty::Always => true,
                DeleteEmpty::Ask => self.ask("Delete empty journal entry/file?")?,
            };
        if !delete {
            return Ok(());
        }
        let here = self.current;
        self.buffer().save_point();
        self.open_entry(true)?;
        let alone = !self.open_entry(true)? || self.current != Some(previous);
        self.current = here;
        self.buffer().restore_point();
        if alone && here != Some(previous) {
            let file = self.buffers[previous].file.clone();
            self.buffers[previous].killed = true;
            self.deleted.push(file);
            self.list_dates()?;
        } else {
            self.buffer().save_point();
            self.open_entry(true)?;
            let buffer = self.buffer();
            let start = buffer.point;
            buffer.outline_end_of_subtree()?;
            let end = buffer.point;
            buffer.delete(start, end);
            let saved = self.current.expect("a buffer");
            self.save(saved)?;
            self.current = here;
            self.buffer().restore_point();
        }
        Ok(())
    }
}
