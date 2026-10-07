//! The commands the server offers, with their titles and default keys.
//!
//! This table is the single source for every editor integration. The
//! server announces it in the `initialize` result (see `manifest`), and
//! `org-lsp manifest` prints it, so an editor needs no list of its own:
//! it runs a command with `workspace/executeCommand` and passes where the
//! cursor is.

use serde_json::{json, Value};

/// Where a key applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Everywhere in the editor.
    Global,
    /// In Org files.
    Org,
    /// In the agenda.
    Agenda,
    /// In the date prompt.
    Calendar,
    /// In a capture buffer.
    Capture,
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Org => "org",
            Scope::Agenda => "agenda",
            Scope::Calendar => "calendar",
            Scope::Capture => "capture",
        }
    }
}

/// A default key for a command.
pub struct Key {
    pub scope: Scope,
    pub key: &'static str,
    /// For modal editors: the key is typed in insert mode, not in normal
    /// mode.
    pub insert: bool,
    /// For modal editors: a key of normal mode that acts as if the cursor
    /// were after the character under it, as `a` types there, and leaves
    /// insert mode on where the command puts the cursor. For the commands
    /// that carry their edit in the result.
    pub append: bool,
    /// The key has a meaning of its own in the editor, which should apply
    /// wherever the command does not: the server then refuses the command
    /// with `notApplicable` in the error data. `<C-a>` still increments a
    /// number that is not part of a timestamp.
    pub native: bool,
}

const fn key(scope: Scope, key: &'static str) -> Key {
    Key { scope, key, native: false, insert: false, append: false }
}

/// How a leader key of Org files is listed for the agenda.
const AGENDA_LEADER: Key = key(Scope::Agenda, "");

const fn native(scope: Scope, key: &'static str) -> Key {
    Key { scope, key, native: true, insert: false, append: false }
}

/// A key of insert mode that keeps its own meaning where the command does
/// not apply.
const fn insert_native(scope: Scope, key: &'static str) -> Key {
    Key { scope, key, native: true, insert: true, append: false }
}

/// A key of normal mode that acts below the cursor's line (`below` in the
/// command's context) and goes on in insert mode.
const fn append(scope: Scope, key: &'static str) -> Key {
    Key { scope, key, native: false, insert: false, append: true }
}

pub struct Command {
    pub id: &'static str,
    pub title: &'static str,
    /// Whether the command changes the text of an Org file. Editors with a
    /// "repeat the last change" key can make these repeatable.
    pub edits: bool,
    /// Default keys as Emacs has them, in its notation (`C-c C-t`, `RET`).
    /// The agenda's keys are the same in every editor and for both
    /// presets: Emacs's, but for those that would take a motion from modal
    /// editors (`J` goes to a date, not `j`; the views are under `v`).
    pub emacs: &'static [Key],
    /// Default keys for modal editors, in Vim's notation (`<CR>`). These
    /// are nvim-orgmode's defaults. None for the agenda: the manifest
    /// gives the Emacs keys there, in Vim's notation.
    pub vim: &'static [Key],
    /// What the command asks the user for before it can run, as the
    /// prompt the editor shows.
    pub input: Option<Input>,
    /// The command reads what the editor would paste: its context has the
    /// text in `clipboard` (Emacs's latest kill).
    pub clipboard: bool,
    /// The command works on an agenda line, which has no key of its own
    /// for it: the vim preset gives it its leader keys there.
    pub in_agenda: bool,
}

/// Input a command asks for.
pub struct Input {
    /// What kind of answer: `date` is read as Org's date prompt reads it.
    pub kind: &'static str,
    pub prompt: &'static str,
}

pub const AGENDA_DISPATCH: &str = "org.agenda.dispatch";
pub const AGENDA_LIST: &str = "org.agenda.list";
pub const AGENDA_TODOS: &str = "org.agenda.todos";
pub const AGENDA_LATER: &str = "org.agenda.later";
pub const AGENDA_EARLIER: &str = "org.agenda.earlier";
pub const AGENDA_TODAY: &str = "org.agenda.today";
pub const AGENDA_DAY: &str = "org.agenda.day";
pub const AGENDA_WEEK: &str = "org.agenda.week";
pub const AGENDA_FORTNIGHT: &str = "org.agenda.fortnight";
pub const AGENDA_MONTH: &str = "org.agenda.month";
pub const AGENDA_YEAR: &str = "org.agenda.year";
pub const AGENDA_REDO: &str = "org.agenda.redo";
pub const AGENDA_GOTO: &str = "org.agenda.goto";
pub const AGENDA_GOTO_OTHER: &str = "org.agenda.goto.other";
pub const AGENDA_SAVE: &str = "org.agenda.save";
pub const AGENDA_GOTO_DATE: &str = "org.agenda.goto.date";
pub const AGENDA_QUERY_ADD: &str = "org.agenda.query.add";
pub const AGENDA_QUERY_SUBTRACT: &str = "org.agenda.query.subtract";
pub const AGENDA_QUERY_ADD_REGEXP: &str = "org.agenda.query.add.regexp";
pub const AGENDA_QUERY_SUBTRACT_REGEXP: &str = "org.agenda.query.subtract.regexp";
pub const AGENDA_LOG: &str = "org.agenda.log";
pub const AGENDA_CLOCKREPORT: &str = "org.agenda.clockreport";
pub const AGENDA_ENTRY_TEXT: &str = "org.agenda.entry.text";
pub const AGENDA_FOLLOW: &str = "org.agenda.follow";
pub const AGENDA_FILTER: &str = "org.agenda.filter";
pub const AGENDA_FILTER_TAG: &str = "org.agenda.filter.tag";
pub const AGENDA_FILTER_CATEGORY: &str = "org.agenda.filter.category";
pub const AGENDA_FILTER_REGEXP: &str = "org.agenda.filter.regexp";
pub const AGENDA_FILTER_EFFORT: &str = "org.agenda.filter.effort";
pub const AGENDA_FILTER_TOP_HEADLINE: &str = "org.agenda.filter.top.headline";
pub const AGENDA_FILTER_REMOVE: &str = "org.agenda.filter.remove";
pub const AGENDA_ARCHIVES: &str = "org.agenda.archives";
pub const AGENDA_ARCHIVES_FILES: &str = "org.agenda.archives.files";
pub const CLOCK_IN: &str = "org.clock.in";
pub const CLOCK_OUT: &str = "org.clock.out";
pub const CLOCK_CANCEL: &str = "org.clock.cancel";
pub const CLOCK_GOTO: &str = "org.clock.goto";
pub const TODO: &str = "org.todo";
pub const SET_TAGS: &str = "org.tags.set";
pub const SET_PROPERTY: &str = "org.property.set";
pub const DELETE_PROPERTY: &str = "org.property.delete";
pub const DELETE_PROPERTY_GLOBALLY: &str = "org.property.delete.globally";
pub const SET_EFFORT: &str = "org.effort.set";
pub const INCREASE_EFFORT: &str = "org.effort.increase";
pub const TOGGLE_ORDERED: &str = "org.ordered.toggle";
pub const CITE_INSERT: &str = "org.cite.insert";
pub const CITE_INSERT_PREFIX: &str = "org.cite.insert.prefix";
pub const TODO_NEXT: &str = "org.todo.next";
pub const TODO_PREVIOUS: &str = "org.todo.previous";
pub const PRIORITY_UP: &str = "org.priority.up";
pub const PRIORITY_DOWN: &str = "org.priority.down";
pub const PRIORITY: &str = "org.priority";
pub const CHECKBOX_TOGGLE: &str = "org.checkbox.toggle";
pub const RETURN: &str = "org.return";
pub const RETURN_ITEM: &str = "org.return.item";
pub const META_RETURN: &str = "org.meta.return";
pub const META_LEFT: &str = "org.meta.left";
pub const META_RIGHT: &str = "org.meta.right";
pub const SHIFT_META_LEFT: &str = "org.shiftmeta.left";
pub const SHIFT_META_RIGHT: &str = "org.shiftmeta.right";
pub const META_UP: &str = "org.meta.up";
pub const META_DOWN: &str = "org.meta.down";
pub const SHIFT_META_UP: &str = "org.shiftmeta.up";
pub const SHIFT_META_DOWN: &str = "org.shiftmeta.down";
pub const TABLE_NEXT_FIELD: &str = "org.table.next.field";
pub const TABLE_PREVIOUS_FIELD: &str = "org.table.previous.field";
pub const TABLE_INSERT_ROW_BELOW: &str = "org.table.insert.row.below";
pub const TABLE_HLINE: &str = "org.table.hline";
pub const TABLE_HLINE_ABOVE: &str = "org.table.hline.above";
pub const TABLE_HLINE_MOVE: &str = "org.table.hline.move";
pub const TABLE_BLANK_FIELD: &str = "org.table.blank.field";
pub const SORT: &str = "org.sort";
pub const SPARSE_TREE: &str = "org.sparse.tree";
pub const CUT_SUBTREE: &str = "org.cut.subtree";
pub const COPY_SUBTREE: &str = "org.copy.subtree";
pub const PASTE_SUBTREE: &str = "org.paste.subtree";
pub const INSERT_TODO_HEADING: &str = "org.insert.todo.heading";
pub const INSERT_HEADING_AFTER: &str = "org.insert.heading.after";
pub const INSERT_TODO_HEADING_AFTER: &str = "org.insert.todo.heading.after";
pub const REFILE: &str = "org.refile";
pub const OPEN: &str = "org.open";
pub const ADD_NOTE: &str = "org.add.note";
pub const TANGLE: &str = "org.babel.tangle";
pub const TABLE_RECALCULATE: &str = "org.table.recalculate";
pub const TABLE_RECALCULATE_LINE: &str = "org.table.recalculate.line";
pub const TABLE_ITERATE: &str = "org.table.iterate";
pub const ARCHIVE_SUBTREE: &str = "org.archive.subtree";
pub const ARCHIVE_SIBLING: &str = "org.archive.sibling";
pub const ARCHIVE_TAG: &str = "org.archive.tag";
pub const ARCHIVE_DEFAULT: &str = "org.archive.default";
pub const ARCHIVE_CONFIRM: &str = "org.archive.confirm";
pub const ARCHIVE_DONE: &str = "org.archive.done";
pub const ARCHIVE_TAG_DONE: &str = "org.archive.tag.done";
pub const CAPTURE: &str = "org.capture";
pub const CAPTURE_FINISH: &str = "org.capture.finish";
pub const CAPTURE_ABORT: &str = "org.capture.abort";
pub const CAPTURE_REFILE: &str = "org.capture.refile";
pub const TIMESTAMP_UP: &str = "org.timestamp.up";
pub const TIMESTAMP_DOWN: &str = "org.timestamp.down";
pub const DATE_LATER: &str = "org.date.later";
pub const DATE_EARLIER: &str = "org.date.earlier";
pub const DATE_LATER_HOURS: &str = "org.date.later.hours";
pub const DATE_EARLIER_HOURS: &str = "org.date.earlier.hours";
pub const DATE_LATER_MINUTES: &str = "org.date.later.minutes";
pub const DATE_EARLIER_MINUTES: &str = "org.date.earlier.minutes";
/// The agenda's commands that move a timestamp.
pub const AGENDA_DATE_COMMANDS: &[&str] =
    &[DATE_LATER, DATE_EARLIER, DATE_LATER_HOURS, DATE_EARLIER_HOURS, DATE_LATER_MINUTES, DATE_EARLIER_MINUTES];
pub const SHIFT_UP: &str = "org.shift.up";
pub const SHIFT_DOWN: &str = "org.shift.down";
pub const SHIFT_LEFT: &str = "org.shift.left";
pub const SHIFT_RIGHT: &str = "org.shift.right";
pub const SCHEDULE: &str = "org.schedule";
pub const DEADLINE: &str = "org.deadline";
pub const TIMESTAMP_INSERT: &str = "org.timestamp.insert";
pub const TIMESTAMP_INSERT_INACTIVE: &str = "org.timestamp.insert.inactive";
pub const TIMESTAMP_CHANGE: &str = "org.timestamp.change";
/// What an answer to a date prompt means, for an editor that shows it
/// while the answer is typed. Not a command of its own for users.
pub const DATE_PREVIEW: &str = "org.date.preview";
pub const CALENDAR_DAY_LATER: &str = "org.calendar.day.later";
pub const CALENDAR_DAY_EARLIER: &str = "org.calendar.day.earlier";
pub const CALENDAR_WEEK_LATER: &str = "org.calendar.week.later";
pub const CALENDAR_WEEK_EARLIER: &str = "org.calendar.week.earlier";
pub const CALENDAR_MONTH_LATER: &str = "org.calendar.month.later";
pub const CALENDAR_MONTH_EARLIER: &str = "org.calendar.month.earlier";
pub const CALENDAR_YEAR_LATER: &str = "org.calendar.year.later";
pub const CALENDAR_YEAR_EARLIER: &str = "org.calendar.year.earlier";
pub const CALENDAR_SCROLL_LATER: &str = "org.calendar.scroll.later";
pub const CALENDAR_SCROLL_EARLIER: &str = "org.calendar.scroll.earlier";
pub const CALENDAR_TODAY: &str = "org.calendar.today";
pub const CALENDAR_SELECT: &str = "org.calendar.select";
pub const CALENDAR_TYPE: &str = "org.calendar.type";
pub const CALENDAR_TIME: &str = "org.calendar.time";
pub const CALENDAR_CANCEL: &str = "org.calendar.cancel";

use Scope::{Agenda, Calendar, Capture, Global, Org};

const fn command(id: &'static str, title: &'static str) -> Command {
    Command { id, title, edits: false, emacs: &[], vim: &[], input: None, clipboard: false, in_agenda: false }
}

const fn edit(id: &'static str, title: &'static str) -> Command {
    Command { id, title, edits: true, emacs: &[], vim: &[], input: None, clipboard: false, in_agenda: false }
}

const fn ask_date(id: &'static str, title: &'static str, prompt: &'static str) -> Command {
    Command { input: Some(Input { kind: "date", prompt }), ..edit(id, title) }
}

pub const COMMANDS: &[Command] = &[
    Command {
        emacs: &[key(Global, "C-c a")],
        vim: &[key(Global, "<Leader>oa")],
        ..command(AGENDA_DISPATCH, "Agenda commands")
    },
    command(AGENDA_LIST, "Agenda for the current week or day"),
    command(AGENDA_TODOS, "List of all TODO entries"),
    Command { emacs: &[key(Agenda, "f")], ..command(AGENDA_LATER, "Agenda: go forward in time") },
    Command { emacs: &[key(Agenda, "b")], ..command(AGENDA_EARLIER, "Agenda: go backward in time") },
    Command { emacs: &[key(Agenda, ".")], ..command(AGENDA_TODAY, "Agenda: go to today") },
    // `org-agenda-manipulate-query`: in a search view, the search string
    // asked for again with a word or regexp to add or subtract started.
    Command { emacs: &[key(Agenda, "[")], ..command(AGENDA_QUERY_ADD, "Agenda: search for another word") },
    Command { emacs: &[key(Agenda, "]")], ..command(AGENDA_QUERY_SUBTRACT, "Agenda: search without a word") },
    Command { emacs: &[key(Agenda, "{")], ..command(AGENDA_QUERY_ADD_REGEXP, "Agenda: search for another regexp") },
    Command { emacs: &[key(Agenda, "}")], ..command(AGENDA_QUERY_SUBTRACT_REGEXP, "Agenda: search without a regexp") },
    Command {
        emacs: &[key(Agenda, "J")],
        input: Some(Input { kind: "date", prompt: "Date" }),
        ..command(AGENDA_GOTO_DATE, "Agenda: go to a date")
    },
    Command { emacs: &[key(Agenda, "v d")], ..command(AGENDA_DAY, "Agenda: day view") },
    Command { emacs: &[key(Agenda, "v w")], ..command(AGENDA_WEEK, "Agenda: week view") },
    Command { emacs: &[key(Agenda, "v t")], ..command(AGENDA_FORTNIGHT, "Agenda: fortnight view") },
    Command { emacs: &[key(Agenda, "v m")], ..command(AGENDA_MONTH, "Agenda: month view") },
    Command { emacs: &[key(Agenda, "v y")], ..command(AGENDA_YEAR, "Agenda: year view") },
    Command { emacs: &[key(Agenda, "r")], ..command(AGENDA_REDO, "Agenda: rebuild") },
    // `org-agenda-switch-to` (RET) shows the entry in the agenda's window
    // or one that shows its file; `org-agenda-goto` (TAB) in another
    // window, made when there is none.
    Command { emacs: &[key(Agenda, "RET")], ..command(AGENDA_GOTO, "Agenda: go to the entry") },
    Command { emacs: &[key(Agenda, "TAB")], ..command(AGENDA_GOTO_OTHER, "Agenda: go to the entry in another window") },
    Command { emacs: &[key(Agenda, "s"), key(Agenda, "C-x C-s")], ..command(AGENDA_SAVE, "Save all Org files") },
    Command { emacs: &[key(Agenda, "v l")], ..command(AGENDA_LOG, "Agenda: log mode, what was closed and clocked") },
    Command {
        emacs: &[key(Agenda, "R"), key(Agenda, "v R")],
        ..command(AGENDA_CLOCKREPORT, "Agenda: the clock report")
    },
    Command {
        emacs: &[key(Agenda, "E"), key(Agenda, "v E")],
        ..command(AGENDA_ENTRY_TEXT, "Agenda: entry text mode, some text of each entry under its line")
    },
    Command {
        emacs: &[key(Agenda, "F")],
        ..command(AGENDA_FOLLOW, "Agenda: follow mode, the cursor's entry in another window")
    },
    // The agenda's filters: lines of entries hidden by tags, category,
    // regexp, effort or top headline, until a new agenda command; with a
    // count, `<` and `=` hide what matches (Emacs's `C-u`).
    Command {
        emacs: &[key(Agenda, "/")],
        input: Some(Input { kind: "text", prompt: "Filter [+cat-tag<0:10-/regexp/]" }),
        ..command(AGENDA_FILTER, "Agenda: filter by tags, categories, effort and regexps")
    },
    Command {
        emacs: &[key(Agenda, "\\")],
        input: Some(Input { kind: "key", prompt: "Filter by tag" }),
        ..command(AGENDA_FILTER_TAG, "Agenda: filter by tag")
    },
    Command { emacs: &[key(Agenda, "<")], ..command(AGENDA_FILTER_CATEGORY, "Agenda: filter by the line's category") },
    Command {
        emacs: &[key(Agenda, "=")],
        input: Some(Input { kind: "text", prompt: "Narrow to entries matching regexp" }),
        ..command(AGENDA_FILTER_REGEXP, "Agenda: filter by a regexp")
    },
    Command {
        emacs: &[key(Agenda, "_")],
        input: Some(Input { kind: "key", prompt: "Effort operator?" }),
        ..command(AGENDA_FILTER_EFFORT, "Agenda: filter by effort")
    },
    Command {
        emacs: &[key(Agenda, "^")],
        ..command(AGENDA_FILTER_TOP_HEADLINE, "Agenda: filter by the line's top headline")
    },
    Command { emacs: &[key(Agenda, "|")], ..command(AGENDA_FILTER_REMOVE, "Agenda: remove all filters") },
    // `org-agenda-archives-mode`: archived trees (`v a`), and the archive
    // files too (`v A`).
    Command { emacs: &[key(Agenda, "v a")], ..command(AGENDA_ARCHIVES, "Agenda: include archived trees") },
    Command {
        emacs: &[key(Agenda, "v A")],
        ..command(AGENDA_ARCHIVES_FILES, "Agenda: include archived trees and archive files")
    },
    // `org-clock-in` and its relatives, in Org files and on agenda lines.
    Command {
        emacs: &[key(Org, "C-c C-x C-i"), key(Agenda, "I")],
        vim: &[key(Org, "<Leader>oxi")],
        ..edit(CLOCK_IN, "Clock in")
    },
    Command {
        emacs: &[key(Org, "C-c C-x C-o"), key(Agenda, "O")],
        vim: &[key(Org, "<Leader>oxo")],
        ..edit(CLOCK_OUT, "Clock out")
    },
    Command {
        emacs: &[key(Org, "C-c C-x C-q"), key(Agenda, "X")],
        vim: &[key(Org, "<Leader>oxq")],
        ..edit(CLOCK_CANCEL, "Cancel the clock")
    },
    Command {
        emacs: &[key(Org, "C-c C-x C-j")],
        vim: &[key(Global, "<Leader>oxj")],
        ..command(CLOCK_GOTO, "Go to the clocked entry")
    },
    // `org-todo`: the state picked by key when the keywords have keys,
    // and else the next one.
    Command {
        emacs: &[key(Org, "C-c C-t"), key(Agenda, "t")],
        vim: &[key(Org, "cit")],
        ..edit(TODO, "Change the TODO state")
    },
    Command { ..edit(TODO_NEXT, "Cycle TODO state forward") },
    Command { vim: &[key(Org, "ciT")], ..edit(TODO_PREVIOUS, "Cycle TODO state backward") },
    Command {
        emacs: &[key(Agenda, "+"), key(Agenda, "S-<up>")],
        vim: &[key(Org, "ciR")],
        ..edit(PRIORITY_UP, "Raise the priority")
    },
    Command {
        emacs: &[key(Agenda, "-"), key(Agenda, "S-<down>")],
        vim: &[key(Org, "cir")],
        ..edit(PRIORITY_DOWN, "Lower the priority")
    },
    Command {
        emacs: &[key(Org, "C-c ,"), key(Agenda, ",")],
        vim: &[key(Org, "<Leader>o,")],
        input: Some(Input { kind: "priority", prompt: "Priority" }),
        ..edit(PRIORITY, "Set the priority")
    },
    // `org-set-property`: the name (completing those of the file and those
    // Org uses), then the value (one of its allowed values, `NAME_ALL`, or
    // any, completing the file's values); in the agenda for the line's
    // entry. `C-c C-c` in a property drawer asks what to do first
    // (`org-property-action`).
    Command {
        emacs: &[key(Org, "C-c C-x p"), key(Agenda, "C-c C-x p")],
        vim: &[key(Org, "<Leader>oxp")],
        input: Some(Input { kind: "text", prompt: "Property" }),
        ..edit(SET_PROPERTY, "Set a property of the entry")
    },
    Command {
        input: Some(Input { kind: "choice", prompt: "Property" }),
        ..edit(DELETE_PROPERTY, "Delete a property of the entry")
    },
    Command {
        input: Some(Input { kind: "text", prompt: "Globally remove property" }),
        ..edit(DELETE_PROPERTY_GLOBALLY, "Delete a property from every entry of the file")
    },
    // `org-set-effort`: one of `Effort_ALL`, or any duration; in the
    // agenda for the line's entry. `org-inc-effort`: the next of them.
    Command {
        emacs: &[key(Org, "C-c C-x e"), key(Agenda, "e")],
        vim: &[key(Org, "<Leader>oxe")],
        input: Some(Input { kind: "text", prompt: "Effort" }),
        ..edit(SET_EFFORT, "Set the effort of the entry")
    },
    Command {
        emacs: &[key(Org, "C-c C-x E")],
        vim: &[key(Org, "<Leader>oxE")],
        ..edit(INCREASE_EFFORT, "Set the effort to the next allowed value")
    },
    Command {
        emacs: &[key(Org, "C-c C-x o")],
        ..edit(TOGGLE_ORDERED, "Toggle whether subtasks must be done in order (ORDERED)")
    },
    // `org-cite-insert` with the `basic` processor: a new citation of keys
    // picked from the bibliography, or on a citation its style, a key
    // before or after the others, or the key at the cursor replaced. With
    // the prefix argument (`C-u C-c C-x @`) the citation or the reference
    // at the cursor goes, and a new citation asks for a style too; no
    // editor binds `C-u` as a prefix, so that is a command of its own.
    Command {
        emacs: &[key(Org, "C-c C-x @")],
        vim: &[key(Org, "<Leader>oic")],
        input: Some(Input { kind: "choice", prompt: "Key" }),
        ..edit(CITE_INSERT, "Insert a citation, or edit the one at the cursor")
    },
    Command {
        vim: &[key(Org, "<Leader>oiC")],
        input: Some(Input { kind: "choice", prompt: "Key" }),
        ..edit(CITE_INSERT_PREFIX, "Delete the citation or reference at the cursor, or insert one with a style")
    },
    // `C-c C-c` on an item (`org-ctrl-c-ctrl-c`): its checkbox toggled and
    // the list repaired (indentation, bullets, numbers, checkboxes); at
    // the start of a list only its first item's box is toggled, and on a
    // statistics cookie the cookies are counted. On a headline,
    // `org-set-tags-command`: the tags picked by key from the tag table
    // (fast tag selection, each key turning its tag on or off until RET),
    // or typed; in the agenda for the line's entry.
    Command {
        emacs: &[key(Org, "C-c C-q")],
        vim: &[key(Org, "<Leader>ot")],
        in_agenda: true,
        input: Some(Input { kind: "key", prompt: "Tags" }),
        ..edit(SET_TAGS, "Set the tags of the headline")
    },
    // `C-c C-c` on an item (`org-ctrl-c-ctrl-c`) for the items below it. The cookies that count
    // checkboxes follow.
    Command {
        emacs: &[key(Org, "C-c C-c")],
        vim: &[key(Org, "<C-Space>")],
        ..edit(CHECKBOX_TOGGLE, "Toggle the checkbox")
    },
    // `org-return`: the result carries the edit and the cursor, for the
    // editor to apply at once, as the key is typed. In a source block's
    // code it refuses, and the editor's own RET applies.
    Command {
        emacs: &[native(Org, "RET")],
        vim: &[insert_native(Org, "<CR>")],
        ..edit(RETURN, "New line, as Org's RET makes it")
    },
    // RET as `return_uses_meta_return` makes it, on a key of its own: on the
    // line of a list item's bullet, the next item, numbered as M-RET
    // numbers it; elsewhere Org's RET.
    Command { vim: &[insert_native(Org, "<S-CR>")], ..edit(RETURN_ITEM, "New line; on a list item, the next item") },
    // M-RET and its neighbours, carried in the result like `org.return`.
    // From normal mode they act below the cursor's line (`below` in the
    // context) and typing goes on in insert mode, as with nvim-orgmode.
    Command {
        emacs: &[key(Org, "M-RET")],
        vim: &[append(Org, "<Leader><CR>")],
        ..edit(META_RETURN, "New heading, item or table row")
    },
    Command {
        emacs: &[key(Org, "M-S-RET")],
        vim: &[append(Org, "<Leader>oiT")],
        ..edit(INSERT_TODO_HEADING, "New TODO heading, or item with a checkbox")
    },
    Command {
        emacs: &[key(Org, "C-RET")],
        vim: &[append(Org, "<Leader>oih")],
        ..edit(INSERT_HEADING_AFTER, "New heading after the subtree")
    },
    Command {
        emacs: &[key(Org, "C-S-RET")],
        vim: &[append(Org, "<Leader>oit")],
        ..edit(INSERT_TODO_HEADING_AFTER, "New TODO heading after the subtree")
    },
    // Structure editing on Org's meta keys: what they do depends on where
    // the cursor is (a table, a headline or a list item here; elsewhere the
    // key's own meaning).
    Command {
        emacs: &[native(Org, "M-<left>")],
        vim: &[native(Org, "<<")],
        ..edit(META_LEFT, "Promote the heading; outdent the item; move the table column left")
    },
    Command {
        emacs: &[native(Org, "M-<right>")],
        vim: &[native(Org, ">>")],
        ..edit(META_RIGHT, "Demote the heading; indent the item, the drawer or the block; move the table column right")
    },
    Command {
        emacs: &[native(Org, "M-S-<left>")],
        vim: &[key(Org, "<s")],
        ..edit(SHIFT_META_LEFT, "Promote the subtree; outdent the item with its children; delete the table column")
    },
    Command {
        emacs: &[native(Org, "M-S-<right>")],
        vim: &[key(Org, ">s")],
        ..edit(SHIFT_META_RIGHT, "Demote the subtree; indent the item with its children; insert a table column")
    },
    Command {
        emacs: &[native(Org, "M-<up>")],
        vim: &[key(Org, "<Leader>oK")],
        ..edit(META_UP, "Move the subtree, the item or the table row up")
    },
    Command {
        emacs: &[native(Org, "M-<down>")],
        vim: &[key(Org, "<Leader>oJ")],
        ..edit(META_DOWN, "Move the subtree, the item or the table row down")
    },
    // The tables of `org-table.el`: `M-S-<up>` and `M-S-<down>` delete and
    // insert a row; `TAB` and `S-TAB` (which the editors send where they
    // cycle folds, on a table line) go from field to field; `C-c -` and
    // `C-c RET` put a rule under the row; `C-c SPC` blanks the field
    // (`orgtbl-mode`'s key, which Org mode 9.8 leaves unbound).
    Command {
        emacs: &[native(Org, "M-S-<up>")],
        vim: &[key(Org, "<Leader>oTd")],
        ..edit(SHIFT_META_UP, "Delete the table row")
    },
    Command {
        emacs: &[native(Org, "M-S-<down>")],
        vim: &[key(Org, "<Leader>oTO")],
        ..edit(SHIFT_META_DOWN, "Insert a table row above")
    },
    Command { vim: &[key(Org, "<Leader>oTo")], ..edit(TABLE_INSERT_ROW_BELOW, "Insert a table row below") },
    edit(TABLE_NEXT_FIELD, "Go to the next table field, making a row after the last"),
    edit(TABLE_PREVIOUS_FIELD, "Go to the previous table field"),
    Command {
        emacs: &[key(Org, "C-c -")],
        vim: &[key(Org, "<Leader>o-")],
        ..edit(TABLE_HLINE, "Insert a rule below the table row")
    },
    edit(TABLE_HLINE_ABOVE, "Insert a rule above the table row"),
    Command {
        emacs: &[key(Org, "C-c RET")],
        vim: &[key(Org, "<Leader>o<CR>")],
        ..edit(TABLE_HLINE_MOVE, "Insert a rule below the table row and go below it")
    },
    Command { vim: &[key(Org, "<Leader>o<Space>")], ..edit(TABLE_BLANK_FIELD, "Blank the table field") },
    Command {
        emacs: &[key(Org, "C-c ^")],
        vim: &[key(Org, "<Leader>o^")],
        input: Some(Input { kind: "key", prompt: "Sort" }),
        ..edit(SORT, "Sort the entries, the items or the table rows")
    },
    Command {
        emacs: &[key(Org, "C-c /")],
        vim: &[key(Org, "<Leader>o/")],
        input: Some(Input { kind: "key", prompt: "Sparse tree" }),
        ..command(SPARSE_TREE, "Show a sparse tree")
    },
    // The subtree to and from the clipboard: the result of cutting and
    // copying has the text to put there in `copy`.
    Command {
        emacs: &[key(Org, "C-c C-x C-w")],
        vim: &[key(Org, "<Leader>osd")],
        ..edit(CUT_SUBTREE, "Cut the subtree")
    },
    Command {
        emacs: &[key(Org, "C-c C-x M-w")],
        vim: &[key(Org, "<Leader>osy")],
        ..command(COPY_SUBTREE, "Copy the subtree")
    },
    Command {
        emacs: &[key(Org, "C-c C-x C-y")],
        vim: &[key(Org, "<Leader>osp")],
        clipboard: true,
        ..edit(PASTE_SUBTREE, "Paste the subtree")
    },
    Command {
        emacs: &[key(Org, "C-c C-w"), key(Agenda, "C-c C-w")],
        vim: &[key(Org, "<Leader>or")],
        input: Some(Input { kind: "choice", prompt: "Refile to" }),
        ..edit(REFILE, "Refile the entry")
    },
    // `org-add-note`: a note in the entry's log, written in a note buffer
    // as a state change's note is; in the agenda the line's entry's
    // (`org-agenda-add-note`).
    Command {
        emacs: &[key(Org, "C-c C-z"), key(Agenda, "z")],
        vim: &[key(Org, "<Leader>ona")],
        input: Some(Input { kind: "note", prompt: "Note" }),
        ..edit(ADD_NOTE, "Add a note to the entry")
    },
    // `org-babel-tangle`: the code of the file's blocks with `:tangle`
    // written to their files.
    Command {
        emacs: &[key(Org, "C-c C-v t"), key(Org, "C-c C-v C-t")],
        vim: &[key(Org, "<Leader>obt")],
        ..command(TANGLE, "Tangle the file's code blocks")
    },
    // `org-table-recalculate`: the formulas of the `#+TBLFM:` line under
    // the table. Emacs's `C-c *` recalculates the cursor's line (and the
    // field formulas); with `C-u`, which no editor binds as a prefix,
    // every line, and with `C-u C-u` until nothing changes. `C-c C-c` on
    // the `#+TBLFM:` line applies that line's formulas to every line.
    Command { vim: &[key(Org, "<Leader>o*")], ..edit(TABLE_RECALCULATE, "Recalculate the table") },
    Command { emacs: &[key(Org, "C-c *")], ..edit(TABLE_RECALCULATE_LINE, "Recalculate the table's current line") },
    edit(TABLE_ITERATE, "Recalculate the table until it does not change"),
    // `org-open-at-point`: a link, a timestamp, a footnote, a tag, or the
    // links of the entry; in the agenda the links of the line's entry
    // (`org-agenda-open-link`).
    Command {
        emacs: &[key(Org, "C-c C-o"), key(Agenda, "C-c C-o")],
        vim: &[key(Org, "<Leader>oo")],
        input: Some(Input { kind: "key", prompt: "Select link to open" }),
        ..command(OPEN, "Open the link or thing at the cursor")
    },
    // `org-archive-subtree` and its relatives, in Org files and on agenda
    // lines (`org-agenda-archive`). The subtree goes to the archive
    // location; another file is written when no editor has it open.
    Command {
        emacs: &[
            key(Org, "C-c C-x C-s"),
            key(Org, "C-c $"),
            key(Agenda, "$"),
            key(Agenda, "C-c C-x C-s"),
            key(Agenda, "C-c $"),
        ],
        vim: &[key(Org, "<Leader>o$")],
        ..edit(ARCHIVE_SUBTREE, "Archive the subtree")
    },
    Command {
        emacs: &[key(Org, "C-c C-x A"), key(Agenda, "C-c C-x A")],
        vim: &[key(Org, "<Leader>oxA")],
        ..edit(ARCHIVE_SIBLING, "Archive the subtree under its Archive sibling")
    },
    Command {
        emacs: &[key(Org, "C-c C-x a"), key(Agenda, "C-c C-x a")],
        vim: &[key(Org, "<Leader>oA")],
        ..edit(ARCHIVE_TAG, "Toggle the ARCHIVE tag")
    },
    Command {
        emacs: &[key(Org, "C-c C-x C-a"), key(Agenda, "C-c C-x C-a")],
        ..edit(ARCHIVE_DEFAULT, "Archive with the default command")
    },
    Command {
        emacs: &[key(Agenda, "a")],
        input: Some(Input { kind: "key", prompt: "Archive this subtree or entry?" }),
        ..edit(ARCHIVE_CONFIRM, "Archive with the default command, once confirmed")
    },
    // Emacs's `C-u C-c C-x C-s` and `C-u C-c C-x a`; no editor binds
    // `C-u` as a prefix, so these are commands without keys.
    Command {
        input: Some(Input { kind: "key", prompt: "Move subtree to archive?" }),
        ..edit(ARCHIVE_DONE, "Archive the subtrees without open TODO items")
    },
    Command {
        input: Some(Input { kind: "key", prompt: "Set ARCHIVE tag?" }),
        ..edit(ARCHIVE_TAG_DONE, "Tag the subtrees without open TODO items as archived")
    },
    Command {
        emacs: &[key(Global, "C-c c")],
        vim: &[key(Global, "<Leader>oc")],
        input: Some(Input { kind: "key", prompt: "Select a capture template" }),
        ..command(CAPTURE, "Capture")
    },
    Command {
        emacs: &[key(Capture, "C-c C-c")],
        vim: &[key(Capture, "<C-c>")],
        ..command(CAPTURE_FINISH, "Capture: finish")
    },
    Command {
        emacs: &[key(Capture, "C-c C-k")],
        vim: &[key(Capture, "<Leader>ok")],
        ..command(CAPTURE_ABORT, "Capture: abort")
    },
    Command {
        emacs: &[key(Capture, "C-c C-w")],
        vim: &[key(Capture, "<Leader>or")],
        input: Some(Input { kind: "choice", prompt: "Refile to" }),
        ..command(CAPTURE_REFILE, "Capture: refile the entry")
    },
    Command { vim: &[native(Org, "<C-a>")], ..edit(TIMESTAMP_UP, "Increase the part of the timestamp at the cursor") },
    Command {
        vim: &[native(Org, "<C-x>")],
        ..edit(TIMESTAMP_DOWN, "Decrease the part of the timestamp at the cursor")
    },
    Command {
        emacs: &[key(Agenda, "S-<right>")],
        vim: &[native(Org, "<S-Up>")],
        ..edit(DATE_LATER, "Move the date one day later")
    },
    Command {
        emacs: &[key(Agenda, "S-<left>")],
        vim: &[native(Org, "<S-Down>")],
        ..edit(DATE_EARLIER, "Move the date one day earlier")
    },
    // In the agenda, by hours and by minutes (Emacs's `C-u S-<right>` and
    // `C-u C-u S-<right>`); `S-<right>` and `S-<left>` right after one of
    // these go on by the same unit, as in Emacs.
    edit(DATE_LATER_HOURS, "Agenda: move the time one hour later"),
    edit(DATE_EARLIER_HOURS, "Agenda: move the time one hour earlier"),
    edit(DATE_LATER_MINUTES, "Agenda: move the time later by the rounding minutes"),
    edit(DATE_EARLIER_MINUTES, "Agenda: move the time earlier by the rounding minutes"),
    // Org's shifted cursor keys, which act on what the cursor is on: a
    // timestamp, the headline, or a list item (its list's bullets cycled,
    // or the cursor to the item before or after, carried in the result as
    // `org.return` is).
    Command {
        emacs: &[native(Org, "S-<up>")],
        ..edit(SHIFT_UP, "Increase the timestamp part, raise the priority, or go to the previous item")
    },
    Command {
        emacs: &[native(Org, "S-<down>")],
        ..edit(SHIFT_DOWN, "Decrease the timestamp part, lower the priority, or go to the next item")
    },
    Command {
        emacs: &[native(Org, "S-<left>")],
        vim: &[native(Org, "<S-Left>")],
        ..edit(SHIFT_LEFT, "Move the date a day earlier, or cycle the TODO state or the list's bullets backward")
    },
    Command {
        emacs: &[native(Org, "S-<right>")],
        vim: &[native(Org, "<S-Right>")],
        ..edit(SHIFT_RIGHT, "Move the date a day later, or cycle the TODO state or the list's bullets forward")
    },
    Command {
        emacs: &[key(Org, "C-c C-s"), key(Agenda, "C-c C-s")],
        vim: &[key(Org, "<Leader>ois")],
        ..ask_date(SCHEDULE, "Schedule the entry", "Schedule")
    },
    Command {
        emacs: &[key(Org, "C-c C-d"), key(Agenda, "C-c C-d")],
        vim: &[key(Org, "<Leader>oid")],
        ..ask_date(DEADLINE, "Set the deadline of the entry", "Deadline")
    },
    Command {
        emacs: &[key(Org, "C-c .")],
        vim: &[key(Org, "<Leader>oi.")],
        ..ask_date(TIMESTAMP_INSERT, "Insert or change a timestamp", "Timestamp")
    },
    Command {
        emacs: &[key(Org, "C-c !")],
        vim: &[key(Org, "<Leader>oi!")],
        ..ask_date(TIMESTAMP_INSERT_INACTIVE, "Insert or change an inactive timestamp", "Inactive timestamp")
    },
    // nvim-orgmode's `cid`; in Emacs, `C-c .` on a timestamp changes it.
    Command {
        vim: &[key(Org, "cid")],
        ..ask_date(TIMESTAMP_CHANGE, "Change the timestamp at the cursor", "Timestamp")
    },
    // The date prompt: the keys of `org-read-date-minibuffer-local-map`
    // and of the calendar Emacs shows with it.
    Command {
        emacs: &[key(Calendar, "S-<right>")],
        vim: &[key(Calendar, "l"), key(Calendar, "<S-Right>")],
        ..command(CALENDAR_DAY_LATER, "Date prompt: one day later")
    },
    Command {
        emacs: &[key(Calendar, "S-<left>")],
        vim: &[key(Calendar, "h"), key(Calendar, "<S-Left>")],
        ..command(CALENDAR_DAY_EARLIER, "Date prompt: one day earlier")
    },
    Command {
        emacs: &[key(Calendar, "S-<down>")],
        vim: &[key(Calendar, "j"), key(Calendar, "<S-Down>")],
        ..command(CALENDAR_WEEK_LATER, "Date prompt: one week later")
    },
    Command {
        emacs: &[key(Calendar, "S-<up>")],
        vim: &[key(Calendar, "k"), key(Calendar, "<S-Up>")],
        ..command(CALENDAR_WEEK_EARLIER, "Date prompt: one week earlier")
    },
    Command {
        emacs: &[key(Calendar, "M-S-<right>")],
        vim: &[key(Calendar, "<M-S-Right>")],
        ..command(CALENDAR_MONTH_LATER, "Date prompt: one month later")
    },
    Command {
        emacs: &[key(Calendar, "M-S-<left>")],
        vim: &[key(Calendar, "<M-S-Left>")],
        ..command(CALENDAR_MONTH_EARLIER, "Date prompt: one month earlier")
    },
    Command {
        emacs: &[key(Calendar, "M-S-<down>")],
        vim: &[key(Calendar, "<M-S-Down>")],
        ..command(CALENDAR_YEAR_LATER, "Date prompt: one year later")
    },
    Command {
        emacs: &[key(Calendar, "M-S-<up>")],
        vim: &[key(Calendar, "<M-S-Up>")],
        ..command(CALENDAR_YEAR_EARLIER, "Date prompt: one year earlier")
    },
    Command {
        emacs: &[key(Calendar, ">")],
        vim: &[key(Calendar, ">")],
        ..command(CALENDAR_SCROLL_LATER, "Date prompt: show the next months")
    },
    Command {
        emacs: &[key(Calendar, "<")],
        vim: &[key(Calendar, "<")],
        ..command(CALENDAR_SCROLL_EARLIER, "Date prompt: show the previous months")
    },
    Command {
        emacs: &[key(Calendar, ".")],
        vim: &[key(Calendar, ".")],
        ..command(CALENDAR_TODAY, "Date prompt: go to today")
    },
    Command {
        emacs: &[key(Calendar, "RET")],
        vim: &[key(Calendar, "<CR>")],
        ..command(CALENDAR_SELECT, "Date prompt: take the day at the cursor")
    },
    Command {
        emacs: &[key(Calendar, "i")],
        vim: &[key(Calendar, "i")],
        input: Some(Input { kind: "date", prompt: "Date+time" }),
        ..command(CALENDAR_TYPE, "Date prompt: type the date")
    },
    // Not Emacs's: its calendar picks the day only, and the time is typed.
    // While it is on, the keys that move by day and week choose the hours
    // or minutes and change them.
    Command {
        emacs: &[key(Calendar, "t")],
        vim: &[key(Calendar, "t")],
        ..command(CALENDAR_TIME, "Date prompt: pick the time, or go back to the day")
    },
    Command {
        emacs: &[key(Calendar, "q"), key(Calendar, "C-g")],
        vim: &[key(Calendar, "q"), key(Calendar, "<Esc>")],
        ..command(CALENDAR_CANCEL, "Date prompt: cancel")
    },
];

/// Every command the server runs: the table's, and those editors call
/// on their own.
pub fn ids() -> Vec<String> {
    COMMANDS.iter().map(|command| command.id).chain([DATE_PREVIEW]).map(str::to_string).collect()
}

/// The input the command `id` asks for, if any.
pub fn input(id: &str) -> Option<&'static Input> {
    COMMANDS.iter().find(|command| command.id == id)?.input.as_ref()
}

/// The table as JSON: a list of
/// `{ "command", "title", "edits", "keys": [{ "key", "in", "preset" }] }`.
/// A key that keeps its own meaning where the command does not apply has
/// `"native": true` as well. A command that asks for something has
/// `"input": { "kind", "prompt" }`; one that reads the clipboard,
/// `"clipboard": true`.
pub fn manifest() -> Value {
    let commands: Vec<Value> = COMMANDS
        .iter()
        .map(|command| {
            // The agenda's keys, in Vim's notation for the vim preset.
            let agenda: Vec<(String, &Key)> = command
                .emacs
                .iter()
                .filter(|key| key.scope == Scope::Agenda)
                .map(|key| (vim_notation(key.key), key))
                .collect();
            // With the vim preset, a command of the agenda's also has the
            // leader keys it has in Org files (`<Leader>ois`), where the
            // hands look for them. Editors that cannot bind the leader
            // leave them out, so the agenda's keys stay the same there.
            let leader: Vec<(String, &Key)> = if agenda.is_empty() && !command.in_agenda {
                Vec::new()
            } else {
                command
                    .vim
                    .iter()
                    .filter(|key| {
                        key.scope == Scope::Org && key.key.starts_with("<Leader>") && !key.insert && !key.append
                    })
                    .map(|key| (key.key.to_string(), &AGENDA_LEADER))
                    .collect()
            };
            let emacs = command.emacs.iter().map(|key| (key.key.to_string(), key));
            let vim = command.vim.iter().map(|key| (key.key.to_string(), key)).chain(agenda).chain(leader);
            let keys: Vec<Value> = [("emacs", emacs.collect::<Vec<_>>()), ("vim", vim.collect())]
                .into_iter()
                .flat_map(|(preset, keys)| {
                    keys.into_iter().map(move |(name, key)| {
                        let mut entry = json!({ "key": name, "in": key.scope.name(), "preset": preset });
                        if key.native {
                            entry["native"] = json!(true);
                        }
                        if key.append {
                            entry["mode"] = json!("append");
                        }
                        if key.insert {
                            entry["mode"] = json!("insert");
                        }
                        entry
                    })
                })
                .collect();
            let mut entry =
                json!({ "command": command.id, "title": command.title, "edits": command.edits, "keys": keys });
            if let Some(input) = &command.input {
                entry["input"] = json!({ "kind": input.kind, "prompt": input.prompt });
            }
            if command.clipboard {
                entry["clipboard"] = json!(true);
            }
            entry
        })
        .collect();
    Value::Array(commands)
}

/// An Emacs key in Vim's notation: `C-c C-s` is `<C-c><C-s>`, `RET` is
/// `<CR>`, `S-<right>` is `<S-Right>` and `v d` is `vd`.
fn vim_notation(emacs: &str) -> String {
    emacs
        .split(' ')
        .map(|key| {
            let mut modifiers = Vec::new();
            let mut base = key;
            while base.len() > 2 && matches!(&base[..2], "C-" | "M-" | "S-") {
                modifiers.push(&base[..1]);
                base = &base[2..];
            }
            let named = match base.trim_start_matches('<').trim_end_matches('>') {
                "RET" => Some("CR".to_string()),
                "TAB" => Some("Tab".to_string()),
                "SPC" => Some("Space".to_string()),
                "ESC" => Some("Esc".to_string()),
                "DEL" => Some("BS".to_string()),
                name if base.starts_with('<') => {
                    let mut chars = name.chars();
                    chars.next().map(|first| first.to_uppercase().chain(chars).collect())
                }
                _ => None,
            };
            match (modifiers.is_empty(), named) {
                (true, None) => base.to_string(),
                (_, named) => {
                    let base = named.unwrap_or_else(|| base.to_string());
                    let prefix: String = modifiers.iter().map(|modifier| format!("{modifier}-")).collect();
                    format!("<{prefix}{base}>")
                }
            }
        })
        .collect()
}

pub fn title(id: &str) -> &'static str {
    COMMANDS.iter().find(|command| command.id == id).map_or("", |command| command.title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_keys_do_not_clash() {
        let mut ids = ids();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), COMMANDS.len() + 1);
        for preset in [|command: &Command| command.emacs, |command: &Command| command.vim] {
            let mut keys: Vec<(&str, &str)> = COMMANDS
                .iter()
                .flat_map(|command| preset(command).iter().map(|key| (key.scope.name(), key.key)))
                .collect();
            let total = keys.len();
            keys.sort();
            keys.dedup();
            assert_eq!(keys.len(), total, "a key is bound twice in one scope");
        }
    }

    #[test]
    fn writes_emacs_keys_in_vim_notation() {
        for (emacs, vim) in [
            ("f", "f"),
            ("J", "J"),
            ("$", "$"),
            ("v d", "vd"),
            ("RET", "<CR>"),
            ("TAB", "<Tab>"),
            ("S-<right>", "<S-Right>"),
            ("C-c C-s", "<C-c><C-s>"),
            ("C-c C-x A", "<C-c><C-x>A"),
            ("C-c C-x C-a", "<C-c><C-x><C-a>"),
            ("C-c $", "<C-c>$"),
            ("C-x C-s", "<C-x><C-s>"),
        ] {
            assert_eq!(vim_notation(emacs), vim, "{emacs}");
        }
    }

    #[test]
    fn the_agenda_has_the_same_keys_for_both_presets() {
        let keys = |preset: &str| -> Vec<(String, String)> {
            let mut keys: Vec<(String, String)> = manifest()
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|command| {
                    let id = command["command"].as_str().unwrap().to_string();
                    command["keys"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|key| key["in"] == "agenda" && key["preset"] == preset)
                        .map(move |key| (id.clone(), key["key"].as_str().unwrap().replace(' ', "")))
                        .collect::<Vec<_>>()
                })
                .collect();
            keys.sort();
            keys
        };
        let emacs = keys("emacs");
        assert!(emacs.contains(&("org.agenda.later".to_string(), "f".to_string())));
        // The same keys, and with the vim preset the leader keys of Org
        // files besides.
        let vim: Vec<(String, String)> =
            keys("vim").into_iter().filter(|(_, key)| !key.starts_with("<Leader>")).collect();
        assert_eq!(emacs.len(), vim.len());
        assert!(keys("vim").contains(&("org.schedule".to_string(), "<Leader>ois".to_string())));
        assert!(COMMANDS.iter().all(|command| command.vim.iter().all(|key| key.scope != Scope::Agenda)));
    }

    #[test]
    fn a_key_is_not_also_the_start_of_a_longer_one() {
        // `v` then `d` in the agenda only works if `v` alone does nothing.
        let keys: Vec<(Scope, &str)> =
            COMMANDS.iter().flat_map(|command| command.emacs.iter().map(|key| (key.scope, key.key))).collect();
        for (scope, key) in &keys {
            let clash =
                keys.iter().any(|(other_scope, other)| other_scope == scope && other.starts_with(&format!("{key} ")));
            assert!(!clash, "`{key}` is a key and a prefix in {scope:?}");
        }
    }
}
