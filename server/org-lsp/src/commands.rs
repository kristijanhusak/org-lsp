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
    /// In an Org file while column view shows.
    Columns,
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Org => "org",
            Scope::Agenda => "agenda",
            Scope::Calendar => "calendar",
            Scope::Capture => "capture",
            Scope::Columns => "columns",
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
pub const AGENDA_INACTIVE: &str = "org.agenda.inactive";
pub const AGENDA_LOG: &str = "org.agenda.log";
pub const AGENDA_HABITS: &str = "org.agenda.habits";
pub const AGENDA_WRITE: &str = "org.agenda.write";
pub const AGENDA_APPT: &str = "org.agenda.appt";
pub const TABLE_TOGGLE_COLUMN_WIDTH: &str = "org.table.toggle.column.width";
pub const COLUMNS: &str = "org.columns";
pub const LINK_PREVIEW: &str = "org.link.preview";
pub const LATEX_PREVIEW: &str = "org.latex.preview";
pub const COLUMNS_QUIT: &str = "org.columns.quit";
pub const COLUMNS_REDO: &str = "org.columns.redo";
pub const COLUMNS_EDIT: &str = "org.columns.edit";
pub const COLUMNS_NEXT: &str = "org.columns.next";
pub const COLUMNS_PREVIOUS: &str = "org.columns.previous";
pub const COLUMNS_SHOW: &str = "org.columns.show";
pub const COLUMNS_ALLOWED: &str = "org.columns.allowed";
pub const COLUMNS_TODO: &str = "org.columns.todo";
pub const COLUMNS_TOGGLE: &str = "org.columns.toggle";
pub const COLUMNS_MOVE_LEFT: &str = "org.columns.move.left";
pub const COLUMNS_MOVE_RIGHT: &str = "org.columns.move.right";
pub const COLUMNS_DELETE: &str = "org.columns.delete";
pub const COLUMNS_NARROW: &str = "org.columns.narrow";
pub const COLUMNS_WIDEN: &str = "org.columns.widen";
pub const COLUMNS_ROW_UP: &str = "org.columns.row.up";
pub const COLUMNS_ROW_DOWN: &str = "org.columns.row.down";
pub const EDIT_SPECIAL: &str = "org.edit.special";
pub const NARROW_SUBTREE: &str = "org.narrow.subtree";
pub const NARROW_BLOCK: &str = "org.narrow.block";
pub const NARROW_WIDEN: &str = "org.narrow.widen";
pub const TREE_INDIRECT: &str = "org.tree.indirect";
pub const TABLE_EDIT_FIELD: &str = "org.table.edit.field";
pub const AGENDA_CLOCKREPORT: &str = "org.agenda.clockreport";
pub const AGENDA_COLUMNS: &str = "org.agenda.columns";
pub const AGENDA_ENTRY_TEXT: &str = "org.agenda.entry.text";
pub const AGENDA_FOLLOW: &str = "org.agenda.follow";
pub const AGENDA_BULK_MARK: &str = "org.agenda.bulk.mark";
pub const AGENDA_BULK_UNMARK: &str = "org.agenda.bulk.unmark";
pub const AGENDA_BULK_UNMARK_ALL: &str = "org.agenda.bulk.unmark.all";
pub const AGENDA_BULK_TOGGLE: &str = "org.agenda.bulk.toggle";
pub const AGENDA_BULK_MARK_ALL: &str = "org.agenda.bulk.mark.all";
pub const AGENDA_BULK_TOGGLE_ALL: &str = "org.agenda.bulk.toggle.all";
pub const AGENDA_BULK_MARK_REGEXP: &str = "org.agenda.bulk.mark.regexp";
pub const AGENDA_BULK_ACTION: &str = "org.agenda.bulk.action";
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
pub const CLOCK_IN_LAST: &str = "org.clock.in.last";
pub const CLOCK_RESOLVE: &str = "org.clock.resolve";
/// The answer to the question about idle time, which the server asks.
pub const CLOCK_RESOLVE_IDLE: &str = "org.clock.resolve.idle";
pub const CLOCK_OUT: &str = "org.clock.out";
pub const CLOCK_CANCEL: &str = "org.clock.cancel";
pub const CLOCK_GOTO: &str = "org.clock.goto";
pub const CLOCK_DISPLAY: &str = "org.clock.display";
pub const CLOCK_REPORT: &str = "org.clock.report";
pub const DBLOCK_UPDATE: &str = "org.dblock.update";
pub const DBLOCK_INSERT: &str = "org.dblock.insert";
pub const ATTACH: &str = "org.attach";
pub const TODO: &str = "org.todo";
pub const SET_TAGS: &str = "org.tags.set";
pub const SET_PROPERTY: &str = "org.property.set";
pub const DELETE_PROPERTY: &str = "org.property.delete";
pub const DELETE_PROPERTY_GLOBALLY: &str = "org.property.delete.globally";
pub const SET_EFFORT: &str = "org.effort.set";
pub const INCREASE_EFFORT: &str = "org.effort.increase";
pub const TOGGLE_ORDERED: &str = "org.ordered.toggle";
pub const LINK_STORE: &str = "org.link.store";
pub const LINK_INSERT: &str = "org.link.insert";
pub const LINK_INSERT_LAST: &str = "org.link.insert.last";
pub const LINK_INSERT_ALL: &str = "org.link.insert.all";
pub const ID_GET_CREATE: &str = "org.id.create";
pub const CITE_INSERT: &str = "org.cite.insert";
pub const CITE_INSERT_PREFIX: &str = "org.cite.insert.prefix";
pub const TODO_NEXT: &str = "org.todo.next";
pub const TODO_PREVIOUS: &str = "org.todo.previous";
pub const TODO_NEXT_SET: &str = "org.todo.nextset";
pub const TODO_PREVIOUS_SET: &str = "org.todo.previousset";
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
pub const REVEAL: &str = "org.reveal";
pub const REVEAL_SIBLINGS: &str = "org.reveal.siblings";
pub const REVEAL_TREE: &str = "org.reveal.tree";
pub const CUT_SUBTREE: &str = "org.cut.subtree";
pub const COPY_SUBTREE: &str = "org.copy.subtree";
pub const PASTE_SUBTREE: &str = "org.paste.subtree";
pub const INSERT_TODO_HEADING: &str = "org.insert.todo.heading";
pub const INSERT_HEADING_AFTER: &str = "org.insert.heading.after";
pub const INSERT_TODO_HEADING_AFTER: &str = "org.insert.todo.heading.after";
pub const REFILE: &str = "org.refile";
pub const REFILE_COPY: &str = "org.refile.copy";
pub const REFILE_GOTO: &str = "org.refile.goto";
pub const REFILE_GOTO_LAST: &str = "org.refile.goto.last";
pub const OPEN: &str = "org.open";
pub const ADD_NOTE: &str = "org.add.note";
pub const TANGLE: &str = "org.babel.tangle";
pub const DETANGLE: &str = "org.babel.detangle";
pub const EXPORT: &str = "org.export.dispatch";
pub const HEADING_NEXT: &str = "org.heading.next";
pub const HEADING_PREVIOUS: &str = "org.heading.previous";
pub const HEADING_FORWARD: &str = "org.heading.forward";
pub const HEADING_BACKWARD: &str = "org.heading.backward";
pub const HEADING_UP: &str = "org.heading.up";
pub const LINK_NEXT: &str = "org.link.next";
pub const LINK_PREVIOUS: &str = "org.link.previous";
pub const GOTO: &str = "org.goto";
pub const FOOTNOTE_ACTION: &str = "org.footnote.action";
pub const FOOTNOTE_MENU: &str = "org.footnote.menu";
pub const TOGGLE_ITEM: &str = "org.toggle.item";
pub const TOGGLE_HEADING: &str = "org.toggle.heading";
pub const TOGGLE_COMMENT: &str = "org.toggle.comment";
pub const EMPHASIZE: &str = "org.emphasize";
pub const STRUCTURE_TEMPLATE: &str = "org.structure.template";
pub const DRAWER_INSERT: &str = "org.drawer.insert";
pub const SUBTREE_CLONE: &str = "org.subtree.clone";
pub const TABLE_COPY_DOWN: &str = "org.table.copy.down";
pub const TABLE_SUM: &str = "org.table.sum";
pub const TABLE_FIELD_INFO: &str = "org.table.field.info";
pub const TABLE_CREATE: &str = "org.table.create";
pub const TABLE_IMPORT: &str = "org.table.import";
pub const TABLE_EXPORT: &str = "org.table.export";
pub const BABEL_NEXT: &str = "org.babel.next";
pub const BABEL_PREVIOUS: &str = "org.babel.previous";
pub const BABEL_GOTO: &str = "org.babel.goto";
pub const BABEL_LOB_INGEST: &str = "org.babel.lob.ingest";
pub const CRYPT_ENCRYPT: &str = "org.crypt.encrypt";
pub const CRYPT_DECRYPT: &str = "org.crypt.decrypt";
pub const CRYPT_ENCRYPT_ALL: &str = "org.crypt.encrypt.all";
pub const CRYPT_DECRYPT_ALL: &str = "org.crypt.decrypt.all";
pub const BLOCK_NEXT: &str = "org.block.next";
pub const BLOCK_PREVIOUS: &str = "org.block.previous";
pub const TIMER_START: &str = "org.timer.start";
pub const TIMER_INSERT: &str = "org.timer.insert";
pub const TIMER_ITEM: &str = "org.timer.item";
pub const TIMER_PAUSE: &str = "org.timer.pause";
pub const TIMER_STOP: &str = "org.timer.stop";
pub const TIMER_SET: &str = "org.timer.set";
pub const BABEL_EXECUTE: &str = "org.babel.execute";
pub const BABEL_EXECUTE_BUFFER: &str = "org.babel.execute.buffer";
pub const BABEL_EXECUTE_SUBTREE: &str = "org.babel.execute.subtree";
pub const BABEL_REMOVE_RESULT: &str = "org.babel.remove_result";
pub const TABLE_RECALCULATE: &str = "org.table.recalculate";
pub const TABLE_RECALCULATE_LINE: &str = "org.table.recalculate.line";
pub const TABLE_ITERATE: &str = "org.table.iterate";
pub const TABLE_FORMULA_DEBUGGER: &str = "org.table.formula.debugger";
pub const ARCHIVE_SUBTREE: &str = "org.archive.subtree";
pub const ARCHIVE_SIBLING: &str = "org.archive.sibling";
pub const ARCHIVE_TAG: &str = "org.archive.tag";
pub const ARCHIVE_DEFAULT: &str = "org.archive.default";
pub const ARCHIVE_CONFIRM: &str = "org.archive.confirm";
pub const ARCHIVE_DONE: &str = "org.archive.done";
pub const ARCHIVE_TAG_DONE: &str = "org.archive.tag.done";
pub const CAPTURE: &str = "org.capture";
pub const CAPTURE_FINISH: &str = "org.capture.finish";
pub const ROAM_NODE_FIND: &str = "org.roam.node.find";
pub const ROAM_NODE_INSERT: &str = "org.roam.node.insert";
pub const ROAM_NODE_RANDOM: &str = "org.roam.node.random";
pub const ROAM_CAPTURE: &str = "org.roam.capture";
pub const ROAM_BUFFER_TOGGLE: &str = "org.roam.buffer.toggle";
pub const ROAM_BUFFER_DEDICATED: &str = "org.roam.buffer.dedicated";
pub const ROAM_BUFFER_REFRESH: &str = "org.roam.buffer.refresh";
pub const ROAM_ALIAS_ADD: &str = "org.roam.alias.add";
pub const ROAM_ALIAS_REMOVE: &str = "org.roam.alias.remove";
pub const ROAM_TAG_ADD: &str = "org.roam.tag.add";
pub const ROAM_TAG_REMOVE: &str = "org.roam.tag.remove";
pub const ROAM_REF_ADD: &str = "org.roam.ref.add";
pub const ROAM_REF_REMOVE: &str = "org.roam.ref.remove";
pub const JOURNAL_NEW_ENTRY: &str = "org.journal.new.entry";
pub const JOURNAL_NEW_DATE_ENTRY: &str = "org.journal.new.date.entry";
pub const JOURNAL_NEW_SCHEDULED_ENTRY: &str = "org.journal.new.scheduled.entry";
pub const JOURNAL_NEXT_ENTRY: &str = "org.journal.next.entry";
pub const JOURNAL_PREVIOUS_ENTRY: &str = "org.journal.previous.entry";
pub const JOURNAL_OPEN_CURRENT: &str = "org.journal.open.current";
pub const JOURNAL_SEARCH: &str = "org.journal.search";
pub const JOURNAL_SEARCH_FOREVER: &str = "org.journal.search.forever";
pub const JOURNAL_SEARCH_FUTURE: &str = "org.journal.search.future";
pub const JOURNAL_SEARCH_FUTURE_SCHEDULED: &str = "org.journal.search.future.scheduled";
pub const JOURNAL_SCHEDULE_VIEW: &str = "org.journal.schedule.view";
pub const JOURNAL_RESCHEDULE: &str = "org.journal.reschedule";
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
pub const SCHEDULE_REMOVE: &str = "org.schedule.remove";
pub const DEADLINE_REMOVE: &str = "org.deadline.remove";
pub const SCHEDULE_DELAY: &str = "org.schedule.delay";
pub const DEADLINE_WARNING: &str = "org.deadline.warning";
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

use Scope::{Agenda, Calendar, Capture, Columns, Global, Org};

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
    // asked for again with a word or regexp to add or subtract started;
    // in an agenda view, the view once more with inactive timestamps.
    Command {
        emacs: &[key(Agenda, "[")],
        ..command(AGENDA_QUERY_ADD, "Agenda: search for another word, or show inactive timestamps")
    },
    Command {
        emacs: &[key(Agenda, "]")],
        ..command(AGENDA_QUERY_SUBTRACT, "Agenda: search without a word, or show inactive timestamps")
    },
    Command {
        emacs: &[key(Agenda, "{")],
        ..command(AGENDA_QUERY_ADD_REGEXP, "Agenda: search for another regexp, or show inactive timestamps")
    },
    Command {
        emacs: &[key(Agenda, "}")],
        ..command(AGENDA_QUERY_SUBTRACT_REGEXP, "Agenda: search without a regexp, or show inactive timestamps")
    },
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
    // `org-habit-toggle-display-in-agenda`; with `C-u`, every habit today
    // or only those due (`org-habit-show-all-today`).
    Command { emacs: &[key(Agenda, "K")], ..command(AGENDA_HABITS, "Agenda: show or hide habits") },
    // `org-agenda-write`
    Command { emacs: &[key(Agenda, "C-x C-w")], ..command(AGENDA_WRITE, "Agenda: write it to a file") },
    // `org-agenda-to-appt`: today's entries with a time, from the agenda
    // files, become appointments of Emacs's `appt.el`, which the server
    // checks each minute; with `C-u`, the list starts over.
    command(AGENDA_APPT, "Add today's appointments from the agenda files"),
    // `org-agenda-columns`: column view of the agenda's entries, through
    // Emacs, with the summaries on the date lines; its keys are column
    // view's, for the values of the entries.
    Command { emacs: &[key(Agenda, "C-c C-x C-c")], ..command(AGENDA_COLUMNS, "Agenda: column view") },
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
    // Bulk actions: lines marked (shown with `>`), then one action for
    // the entries of all of them.
    Command { emacs: &[key(Agenda, "m")], ..command(AGENDA_BULK_MARK, "Agenda: mark the entry for a bulk action") },
    Command { emacs: &[key(Agenda, "u")], ..command(AGENDA_BULK_UNMARK, "Agenda: unmark the entry") },
    Command { emacs: &[key(Agenda, "U")], ..command(AGENDA_BULK_UNMARK_ALL, "Agenda: unmark all entries") },
    Command { emacs: &[key(Agenda, "M-m")], ..command(AGENDA_BULK_TOGGLE, "Agenda: toggle the mark of the entry") },
    Command { emacs: &[key(Agenda, "*")], ..command(AGENDA_BULK_MARK_ALL, "Agenda: mark all entries") },
    Command {
        emacs: &[key(Agenda, "M-*")],
        ..command(AGENDA_BULK_TOGGLE_ALL, "Agenda: toggle the marks of all entries")
    },
    Command {
        emacs: &[key(Agenda, "%")],
        input: Some(Input { kind: "text", prompt: "Mark entries matching regexp" }),
        ..command(AGENDA_BULK_MARK_REGEXP, "Agenda: mark the entries matching a regexp")
    },
    Command { emacs: &[key(Agenda, "B")], ..command(AGENDA_BULK_ACTION, "Agenda: an action on the marked entries") },
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
    // `org-agenda-include-inactive-timestamps` for one rebuilding (`v [`).
    Command { emacs: &[key(Agenda, "v [")], ..command(AGENDA_INACTIVE, "Agenda: show inactive timestamps once") },
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
    // `org-clock-in-last`: the entry clocked last, or with a count one of
    // the history (`org-clock-in` with a count asks the same).
    Command {
        emacs: &[key(Org, "C-c C-x C-x")],
        vim: &[key(Org, "<Leader>oxx")],
        ..edit(CLOCK_IN_LAST, "Clock in the last clocked task")
    },
    // `org-resolve-clocks`: a clock left running beside the clock.
    Command { vim: &[key(Org, "<Leader>oxR")], ..edit(CLOCK_RESOLVE, "Resolve a dangling clock") },
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
    // `org-clock-display`: the times show as inlay hints until the next
    // change, or `C-c C-c`.
    Command {
        emacs: &[key(Org, "C-c C-x C-d")],
        vim: &[key(Org, "<Leader>oxd")],
        ..command(CLOCK_DISPLAY, "Show the time clocked under each headline")
    },
    // Clock tables: `org-clock-report` inserts one or fills it in again,
    // the first of the file with a count; `org-dblock-update` fills in the
    // dynamic block the cursor is in, all of the file's with a count.
    Command {
        emacs: &[key(Org, "C-c C-x C-r")],
        vim: &[key(Org, "<Leader>oxr")],
        ..edit(CLOCK_REPORT, "Insert or update a clock table")
    },
    Command {
        emacs: &[key(Org, "C-c C-x C-u")],
        vim: &[key(Org, "<Leader>oxu")],
        ..edit(DBLOCK_UPDATE, "Update the dynamic block")
    },
    // `org-dynamic-block-insert-dblock`: a clock table, or a column view
    // (`org-columns-insert-dblock`, in Emacs).
    Command {
        emacs: &[key(Org, "C-c C-x x")],
        vim: &[key(Org, "<Leader>oix")],
        input: Some(Input { kind: "choice", prompt: "Dynamic block" }),
        ..edit(DBLOCK_INSERT, "Insert a dynamic block")
    },
    // `org-attach` (`C-c C-a`): Org's dispatcher of attachment commands,
    // run by Emacs.
    Command {
        emacs: &[key(Org, "C-c C-a")],
        vim: &[key(Org, "<Leader>oxa")],
        input: Some(Input { kind: "key", prompt: "Attachments" }),
        ..edit(ATTACH, "Attachments of the entry")
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
    // `org-shiftcontrolright` and `org-shiftcontrolleft` on a headline:
    // `org-todo` with `nextset` and `previousset`.
    Command {
        emacs: &[key(Org, "C-S-<right>"), key(Agenda, "C-S-<right>")],
        ..edit(TODO_NEXT_SET, "Switch to the next TODO keyword set")
    },
    Command {
        emacs: &[key(Org, "C-S-<left>"), key(Agenda, "C-S-<left>")],
        ..edit(TODO_PREVIOUS_SET, "Switch to the previous TODO keyword set")
    },
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
    // `org-reveal`: the context of the cursor shown, the other folds left
    // as they are; the result has `sparse` with `keep`. With `C-u`, which
    // no editor binds as a prefix, each level with its entry and children
    // (`canonical`), with `C-u C-u` the parent's whole tree.
    Command {
        emacs: &[key(Org, "C-c C-r")],
        vim: &[key(Org, "<Leader>oR")],
        ..command(REVEAL, "Show the context of the cursor")
    },
    command(REVEAL_SIBLINGS, "Show the context of the cursor, each level with its children"),
    command(REVEAL_TREE, "Show the whole tree of the cursor's parent"),
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
    // `org-refile-copy`: as refiling, the entry staying where it was too.
    Command {
        emacs: &[key(Org, "C-c M-w")],
        vim: &[key(Org, "<Leader>oy")],
        input: Some(Input { kind: "choice", prompt: "Copy to" }),
        ..edit(REFILE_COPY, "Copy the entry to a refile target")
    },
    // `org-refile` with `C-u` and `C-u C-u`: to a target, or to where the
    // last refile put its entry, without moving anything.
    Command { input: Some(Input { kind: "choice", prompt: "Goto" }), ..command(REFILE_GOTO, "Go to a refile target") },
    command(REFILE_GOTO_LAST, "Go to where the last refile put its entry"),
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
    // `org-babel-detangle`, by Emacs: the code of a file tangled from this
    // one (with `:comments link`) put back into the blocks it came from.
    // Emacs runs it in the tangled file; here it asks which one, among
    // those this file tangles to.
    Command {
        vim: &[key(Org, "<Leader>obd")],
        input: Some(Input { kind: "choice", prompt: "Detangle" }),
        ..command(DETANGLE, "Put the code of a tangled file back into its blocks")
    },
    // Babel's evaluation, by Emacs itself (`emacs`): the block or call at
    // the cursor (`org-babel-execute-maybe`; `C-c C-c` there too), the
    // file's (`org-babel-execute-buffer`), the subtree's
    // (`org-babel-execute-subtree`), and taking out the block's results
    // (`org-babel-remove-result-one-or-many`).
    Command {
        emacs: &[key(Org, "C-c C-v e"), key(Org, "C-c C-v C-e")],
        vim: &[key(Org, "<Leader>obe")],
        ..command(BABEL_EXECUTE, "Evaluate the code block or call at the cursor through Emacs")
    },
    Command {
        emacs: &[key(Org, "C-c C-v b"), key(Org, "C-c C-v C-b")],
        vim: &[key(Org, "<Leader>obb")],
        ..command(BABEL_EXECUTE_BUFFER, "Evaluate the file's code blocks through Emacs")
    },
    Command {
        emacs: &[key(Org, "C-c C-v s"), key(Org, "C-c C-v C-s")],
        vim: &[key(Org, "<Leader>obs")],
        ..command(BABEL_EXECUTE_SUBTREE, "Evaluate the subtree's code blocks through Emacs")
    },
    Command {
        emacs: &[key(Org, "C-c C-v k")],
        vim: &[key(Org, "<Leader>obk")],
        ..command(BABEL_REMOVE_RESULT, "Remove the results of the code block at the cursor")
    },
    // Moving about: `org-next-visible-heading` and its relatives, by
    // headings as Org finds them (`^\*+ `); the count repeats them. The
    // result carries the cursor, as `org.return`'s does.
    Command {
        emacs: &[key(Org, "C-c C-n")],
        vim: &[key(Org, "}")],
        ..command(HEADING_NEXT, "Move to the next heading")
    },
    Command {
        emacs: &[key(Org, "C-c C-p")],
        vim: &[key(Org, "{")],
        ..command(HEADING_PREVIOUS, "Move to the previous heading")
    },
    Command {
        emacs: &[key(Org, "C-c C-f")],
        vim: &[key(Org, "]]")],
        ..command(HEADING_FORWARD, "Move to the next heading of the same level")
    },
    Command {
        emacs: &[key(Org, "C-c C-b")],
        vim: &[key(Org, "[[")],
        ..command(HEADING_BACKWARD, "Move to the previous heading of the same level")
    },
    Command {
        emacs: &[key(Org, "C-c C-u")],
        vim: &[key(Org, "g{")],
        ..command(HEADING_UP, "Move to the parent heading")
    },
    // `org-next-link` and `org-previous-link`: after a search that found
    // nothing, the same command again starts from the other end.
    Command {
        emacs: &[key(Org, "C-c C-x C-n")],
        vim: &[key(Org, "]l")],
        ..command(LINK_NEXT, "Move to the next link")
    },
    Command {
        emacs: &[key(Org, "C-c C-x C-p")],
        vim: &[key(Org, "[l")],
        ..command(LINK_PREVIOUS, "Move to the previous link")
    },
    // `org-goto` with its `outline-path-completion` interface: the file's
    // headings by their outline path, to `org_goto_max_level`.
    Command {
        emacs: &[key(Org, "C-c C-j")],
        vim: &[key(Org, "<Leader>oj")],
        input: Some(Input { kind: "choice", prompt: "Goto" }),
        ..command(GOTO, "Go to a heading of the file")
    },
    // Run by Emacs itself (`emacs`): what it changes comes back as an edit,
    // and the cursor goes where Emacs leaves the point.
    //
    // `org-footnote-action`: a new footnote (the reference at the cursor,
    // the definition in the `Footnotes` section, the cursor there), or from
    // a reference to its definition and back.
    Command {
        emacs: &[key(Org, "C-c C-x f")],
        vim: &[key(Org, "<Leader>of")],
        ..command(FOOTNOTE_ACTION, "Make a footnote, or go between a footnote and its definition")
    },
    // `org-footnote-action` with the prefix argument: its menu.
    Command {
        vim: &[key(Org, "<Leader>oF")],
        input: Some(Input { kind: "key", prompt: "Footnotes" }),
        ..command(FOOTNOTE_MENU, "Sort, renumber, normalize or delete footnotes")
    },
    // `org-toggle-item` and `org-toggle-heading`: also `C-c -` and `C-c *`
    // off a table.
    command(TOGGLE_ITEM, "Turn the line into an item, or the item into a line"),
    command(TOGGLE_HEADING, "Turn the line into a heading, or the heading into a line"),
    Command {
        emacs: &[key(Org, "C-c ;")],
        vim: &[key(Org, "<Leader>o;")],
        ..command(TOGGLE_COMMENT, "Toggle the COMMENT keyword of the heading")
    },
    Command {
        emacs: &[key(Org, "C-c C-x C-f")],
        vim: &[key(Org, "<Leader>oie")],
        input: Some(Input { kind: "key", prompt: "Emphasis marker or tag" }),
        ..command(EMPHASIZE, "Insert emphasis markers")
    },
    Command {
        emacs: &[key(Org, "C-c C-,")],
        vim: &[key(Org, "<Leader>oib")],
        input: Some(Input { kind: "key", prompt: "Select a key" }),
        ..command(STRUCTURE_TEMPLATE, "Insert a block (structure template)")
    },
    Command {
        emacs: &[key(Org, "C-c C-x d")],
        vim: &[key(Org, "<Leader>oiD")],
        input: Some(Input { kind: "text", prompt: "Drawer" }),
        ..command(DRAWER_INSERT, "Insert a drawer")
    },
    Command {
        emacs: &[key(Org, "C-c C-x c")],
        vim: &[key(Org, "<Leader>osc")],
        input: Some(Input { kind: "text", prompt: "Number of clones to produce" }),
        ..command(SUBTREE_CLONE, "Clone the subtree, shifting its dates")
    },
    // Tables, by Org in Emacs: `org-table-copy-down` (`S-RET`),
    // `org-table-sum`, `org-table-field-info` and `org-table-create`.
    Command {
        emacs: &[native(Org, "S-RET")],
        vim: &[key(Org, "<Leader>oD")],
        ..command(TABLE_COPY_DOWN, "Copy the field down to the next row")
    },
    Command {
        emacs: &[key(Org, "C-c +")],
        vim: &[key(Org, "<Leader>o+")],
        ..command(TABLE_SUM, "Sum the numbers of the table column")
    },
    Command {
        emacs: &[key(Org, "C-c ?")],
        vim: &[key(Org, "<Leader>o?")],
        ..command(TABLE_FIELD_INFO, "Show the references of the table field")
    },
    Command {
        emacs: &[key(Org, "C-c |")],
        vim: &[key(Org, "<Leader>oi<Bar>")],
        input: Some(Input { kind: "text", prompt: "Table size Columns x Rows" }),
        ..command(TABLE_CREATE, "Insert an empty table")
    },
    // `org-table-import` and `org-table-export`, in Emacs: a file of
    // tab- or comma-separated values made a table at the cursor, and the
    // table written to a file in a format of `orgtbl-to-…`.
    Command {
        input: Some(Input { kind: "text", prompt: "Import file" }),
        ..edit(TABLE_IMPORT, "Import a file as a table")
    },
    Command {
        input: Some(Input { kind: "text", prompt: "Export table to" }),
        ..command(TABLE_EXPORT, "Export the table to a file")
    },
    // Moving between blocks, by Org in Emacs: source blocks
    // (`org-babel-next-src-block`, `-previous-`, `org-babel-goto-named-src-block`)
    // and any block (`org-next-block`, `org-previous-block`).
    Command {
        emacs: &[key(Org, "C-c C-v n"), key(Org, "C-c C-v C-n")],
        vim: &[key(Org, "<Leader>obn")],
        ..command(BABEL_NEXT, "Move to the next code block")
    },
    Command {
        emacs: &[key(Org, "C-c C-v p"), key(Org, "C-c C-v C-p")],
        vim: &[key(Org, "<Leader>obp")],
        ..command(BABEL_PREVIOUS, "Move to the previous code block")
    },
    Command {
        emacs: &[key(Org, "C-c C-v g")],
        vim: &[key(Org, "<Leader>obg")],
        input: Some(Input { kind: "choice", prompt: "source-block name" }),
        ..command(BABEL_GOTO, "Go to a named code block")
    },
    // `org-babel-lob-ingest`: the file's named code blocks into the Library
    // of Babel of the server's Emacs, for `#+CALL:` lines in any file.
    Command {
        emacs: &[key(Org, "C-c C-v i")],
        vim: &[key(Org, "<Leader>obi")],
        ..command(BABEL_LOB_INGEST, "Add the file's named code blocks to the Library of Babel")
    },
    Command { emacs: &[key(Org, "C-c M-f")], ..command(BLOCK_NEXT, "Move to the next block") },
    Command { emacs: &[key(Org, "C-c M-b")], ..command(BLOCK_PREVIOUS, "Move to the previous block") },
    // `org-edit-special`: what Org edits in a buffer of its own (a code
    // block, an example or export block, a footnote's definition, LaTeX…)
    // in a file of its own, written back each time it is saved
    // (`org-edit-src-save`); at a table its formulas, installed each time
    // (`org-table-fedit-finish`); at `#+INCLUDE:` the file included, at a date
    // or a link what `C-c .`, `C-c C-d`, `C-c C-s` or `C-c C-o` do there.
    Command {
        emacs: &[key(Org, "C-c '")],
        vim: &[key(Org, "<Leader>o'")],
        ..command(EDIT_SPECIAL, "Edit the block in a file of its own")
    },
    // Narrowing (`org-narrow-to-subtree`, `org-narrow-to-block`) and
    // `org-tree-to-indirect-buffer`: the subtree or block at the cursor in
    // a file of its own, all there is to see while it is edited, written
    // back each time it is saved.
    Command {
        emacs: &[key(Org, "C-x n s")],
        vim: &[key(Org, "<Leader>ons")],
        ..command(NARROW_SUBTREE, "Narrow to the subtree: it in a file of its own")
    },
    Command {
        emacs: &[key(Org, "C-x n b")],
        vim: &[key(Org, "<Leader>onb")],
        ..command(NARROW_BLOCK, "Narrow to the block: it in a file of its own")
    },
    // `widen` (`C-x n w`) in a narrowed file, and `C-c '` there as in an
    // edit buffer (`org-edit-src-exit`): what it says goes back, and it
    // closes.
    Command {
        emacs: &[key(Org, "C-x n w")],
        vim: &[key(Org, "<Leader>onw")],
        ..command(NARROW_WIDEN, "Widen: the narrowed file written back and closed")
    },
    Command {
        emacs: &[key(Org, "C-c C-x b")],
        vim: &[key(Org, "<Leader>oxb")],
        ..command(TREE_INDIRECT, "Show the subtree in a file of its own")
    },
    // `org-table-edit-field`: the field at the cursor in a file of its own,
    // put back in the table, realigned, each time it is saved.
    Command {
        emacs: &[key(Org, "C-c `")],
        vim: &[key(Org, "<Leader>o`")],
        ..command(TABLE_EDIT_FIELD, "Edit the table field in a file of its own")
    },
    // `org-crypt`, through Emacs and its GnuPG: an entry's text encrypted
    // for `org-crypt-key` (or its `CRYPTKEY` property), or with a
    // passphrase, which GnuPG's own pinentry asks for; the entries tagged
    // `crypt` at once. Emacs binds no keys to them.
    edit(CRYPT_ENCRYPT, "Encrypt the entry"),
    edit(CRYPT_DECRYPT, "Decrypt the entry"),
    edit(CRYPT_ENCRYPT_ALL, "Encrypt the entries tagged crypt"),
    edit(CRYPT_DECRYPT_ALL, "Decrypt the encrypted entries"),
    // Column view (`org-columns`), through Emacs: the properties of the
    // entries as a table over their headlines, the header above; on the
    // whole file with `C-u` or before the first headline. While it shows,
    // its keys act on the column picked (the editor keeps which).
    Command { emacs: &[key(Org, "C-c C-x C-c")], vim: &[key(Org, "<Leader>oxc")], ..command(COLUMNS, "Column view") },
    // `org-link-preview` and `org-latex-preview`: images in place of the
    // links to image files and of the LaTeX fragments, for an editor that
    // can show them.
    Command {
        emacs: &[key(Org, "C-c C-x C-v")],
        vim: &[key(Org, "<Leader>oxv")],
        ..command(LINK_PREVIEW, "Preview the images of links")
    },
    Command {
        emacs: &[key(Org, "C-c C-x C-l")],
        vim: &[key(Org, "<Leader>oxl")],
        ..command(LATEX_PREVIEW, "Preview LaTeX fragments")
    },
    Command { emacs: &[key(Columns, "q")], vim: &[key(Columns, "q")], ..command(COLUMNS_QUIT, "Column view: quit") },
    Command {
        emacs: &[key(Columns, "g"), key(Columns, "r")],
        // `g` would wait for `gg`.
        vim: &[key(Columns, "r")],
        ..command(COLUMNS_REDO, "Column view: recompute")
    },
    Command {
        emacs: &[key(Columns, "e")],
        vim: &[key(Columns, "e")],
        ..edit(COLUMNS_EDIT, "Column view: edit the value")
    },
    Command {
        emacs: &[key(Columns, "n"), key(Columns, "S-<right>")],
        vim: &[key(Columns, "n"), key(Columns, "<S-Right>")],
        ..edit(COLUMNS_NEXT, "Column view: the next allowed value")
    },
    Command {
        emacs: &[key(Columns, "p"), key(Columns, "S-<left>")],
        vim: &[key(Columns, "p"), key(Columns, "<S-Left>")],
        ..edit(COLUMNS_PREVIOUS, "Column view: the previous allowed value")
    },
    Command {
        emacs: &[key(Columns, "v")],
        vim: &[key(Columns, "v")],
        ..command(COLUMNS_SHOW, "Column view: show the value")
    },
    Command {
        emacs: &[key(Columns, "a")],
        vim: &[key(Columns, "a")],
        ..edit(COLUMNS_ALLOWED, "Column view: edit the allowed values")
    },
    Command {
        emacs: &[key(Columns, "C-c C-t")],
        vim: &[key(Columns, "t")],
        ..edit(COLUMNS_TODO, "Column view: change the TODO state")
    },
    Command {
        emacs: &[key(Columns, "C-c C-c")],
        vim: &[key(Columns, "<CR>")],
        ..edit(COLUMNS_TOGGLE, "Column view: toggle a checkbox, or quit")
    },
    Command {
        emacs: &[key(Columns, "M-<left>")],
        vim: &[key(Columns, "<M-Left>")],
        ..edit(COLUMNS_MOVE_LEFT, "Column view: move the column left")
    },
    Command {
        emacs: &[key(Columns, "M-<right>")],
        vim: &[key(Columns, "<M-Right>")],
        ..edit(COLUMNS_MOVE_RIGHT, "Column view: move the column right")
    },
    Command {
        emacs: &[key(Columns, "M-S-<left>")],
        vim: &[key(Columns, "<M-S-Left>")],
        ..edit(COLUMNS_DELETE, "Column view: delete the column")
    },
    Command {
        emacs: &[key(Columns, "<")],
        vim: &[key(Columns, "<")],
        ..edit(COLUMNS_NARROW, "Column view: make the column narrower")
    },
    Command {
        emacs: &[key(Columns, ">")],
        vim: &[key(Columns, ">")],
        ..edit(COLUMNS_WIDEN, "Column view: make the column wider")
    },
    Command {
        emacs: &[key(Columns, "M-<up>")],
        vim: &[key(Columns, "<M-Up>")],
        ..edit(COLUMNS_ROW_UP, "Column view: move the entry up")
    },
    Command {
        emacs: &[key(Columns, "M-<down>")],
        vim: &[key(Columns, "<M-Down>")],
        ..edit(COLUMNS_ROW_DOWN, "Column view: move the entry down")
    },
    // Timers, kept by Org in Emacs (`org-timer.el`): the relative timer
    // (`org-timer-start`, `org-timer`, `org-timer-item`), pausing and
    // stopping it, and the countdown (`org-timer-set-timer`), whose end the
    // server says.
    Command {
        emacs: &[key(Org, "C-c C-x 0")],
        vim: &[key(Org, "<Leader>ox0")],
        ..command(TIMER_START, "Start the relative timer")
    },
    Command {
        emacs: &[key(Org, "C-c C-x .")],
        vim: &[key(Org, "<Leader>ox.")],
        ..command(TIMER_INSERT, "Insert the timer's value")
    },
    Command {
        emacs: &[key(Org, "C-c C-x -")],
        vim: &[key(Org, "<Leader>ox-")],
        ..command(TIMER_ITEM, "Insert a list item with the timer's value")
    },
    Command {
        emacs: &[key(Org, "C-c C-x ,")],
        vim: &[key(Org, "<Leader>ox,")],
        ..command(TIMER_PAUSE, "Pause or continue the timer")
    },
    Command {
        emacs: &[key(Org, "C-c C-x _")],
        vim: &[key(Org, "<Leader>ox_")],
        ..command(TIMER_STOP, "Stop the timer")
    },
    Command {
        emacs: &[key(Org, "C-c C-x ;")],
        vim: &[key(Org, "<Leader>ox;")],
        input: Some(Input { kind: "text", prompt: "How much time left? (minutes or h:mm:ss)" }),
        ..command(TIMER_SET, "Set a countdown timer")
    },
    // `org-export-dispatch`: Org's export menu, from the backends Emacs
    // has, run by Emacs itself (`emacs`); the file or buffer it makes is
    // shown.
    Command {
        emacs: &[key(Org, "C-c C-e")],
        vim: &[key(Org, "<Leader>oe")],
        input: Some(Input { kind: "key", prompt: "Export command" }),
        ..command(EXPORT, "Export the file through Emacs")
    },
    // `org-table-toggle-column-width` (`C-c TAB` in a table): the column
    // at the cursor shrunk to its width cookie (`<5>`), or to nothing but
    // `…`, or shown whole again; before the first column, the columns asked
    // for. With `C-u` every column with a cookie shrunk and the others
    // shown (`org-table-shrink`), with `C-u C-u` all shown
    // (`org-table-expand`).
    Command {
        emacs: &[key(Org, "C-c TAB")],
        vim: &[key(Org, "<Leader>o<Tab>")],
        ..command(TABLE_TOGGLE_COLUMN_WIDTH, "Shrink or expand the table column")
    },
    // `org-table-recalculate`: the formulas of the `#+TBLFM:` line under
    // the table. Emacs's `C-c *` recalculates the cursor's line (and the
    // field formulas); with `C-u`, which no editor binds as a prefix,
    // every line, and with `C-u C-u` until nothing changes. `C-c C-c` on
    // the `#+TBLFM:` line applies that line's formulas to every line.
    Command { vim: &[key(Org, "<Leader>o*")], ..edit(TABLE_RECALCULATE, "Recalculate the table") },
    Command { emacs: &[key(Org, "C-c *")], ..edit(TABLE_RECALCULATE_LINE, "Recalculate the table's current line") },
    edit(TABLE_ITERATE, "Recalculate the table until it does not change"),
    // `org-table-toggle-formula-debugger`: while it is on, recalculating
    // goes through Emacs, which asks after each formula whether to go on,
    // its substitution history with the question.
    Command {
        emacs: &[key(Org, "C-c {")],
        vim: &[key(Org, "<Leader>o{")],
        ..command(TABLE_FORMULA_DEBUGGER, "Turn the formula debugger on or off")
    },
    // `org-open-at-point`: a link, a timestamp, a footnote, a tag, or the
    // links of the entry; in the agenda the links of the line's entry
    // (`org-agenda-open-link`).
    Command {
        emacs: &[key(Org, "C-c C-o"), key(Agenda, "C-c C-o")],
        vim: &[key(Org, "<Leader>oo")],
        input: Some(Input { kind: "key", prompt: "Select link to open" }),
        ..command(OPEN, "Open the link or thing at the cursor")
    },
    // `org-store-link`: a link to the place at the cursor, or to the
    // entry of an agenda line, kept for `org.link.insert`. With `C-u` (a
    // count of 4) `org_link_context_for_files` is turned around.
    Command {
        emacs: &[key(Org, "C-c l"), key(Agenda, "C-c l")],
        vim: &[key(Org, "<Leader>ols")],
        ..command(LINK_STORE, "Store a link to the place at the cursor")
    },
    // `org-insert-link`: a stored link or one typed, or the link at the
    // cursor edited; with `C-u` a link to a file, with `C-u C-u` by its
    // absolute name, with `C-u C-u C-u` the link stays stored.
    Command {
        emacs: &[key(Org, "C-c C-l")],
        vim: &[key(Org, "<Leader>oli")],
        input: Some(Input { kind: "text", prompt: "Insert link" }),
        ..edit(LINK_INSERT, "Insert a link, or edit the one at the cursor")
    },
    // `org-insert-last-stored-link` and `org-insert-all-links`.
    Command { emacs: &[key(Org, "C-c M-l")], ..edit(LINK_INSERT_LAST, "Insert the last stored link") },
    Command { emacs: &[key(Org, "C-c C-M-l")], ..edit(LINK_INSERT_ALL, "Insert all stored links") },
    // `org-id-get-create`: the entry gets an ID property unless it has one.
    edit(ID_GET_CREATE, "Create an ID for the entry"),
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
        clipboard: true,
        ..command(CAPTURE, "Capture")
    },
    Command {
        emacs: &[key(Capture, "C-c C-c")],
        vim: &[key(Capture, "<C-c>")],
        ..command(CAPTURE_FINISH, "Capture: finish")
    },
    Command {
        emacs: &[key(Global, "C-c n f")],
        vim: &[key(Org, "<Leader>nf")],
        input: Some(Input { kind: "text", prompt: "Node" }),
        ..command(ROAM_NODE_FIND, "Roam: find a node")
    },
    Command {
        emacs: &[key(Org, "C-c n i")],
        vim: &[key(Org, "<Leader>ni")],
        input: Some(Input { kind: "text", prompt: "Node" }),
        clipboard: true,
        ..edit(ROAM_NODE_INSERT, "Roam: insert a link to a node")
    },
    Command {
        emacs: &[key(Global, "C-c n c")],
        vim: &[key(Org, "<Leader>nc")],
        input: Some(Input { kind: "text", prompt: "Node" }),
        clipboard: true,
        ..command(ROAM_CAPTURE, "Roam: capture to a node")
    },
    command(ROAM_NODE_RANDOM, "Roam: visit a random node"),
    Command {
        emacs: &[key(Global, "C-c n l")],
        vim: &[key(Org, "<Leader>nl")],
        ..command(ROAM_BUFFER_TOGGLE, "Roam: show or hide the org-roam buffer")
    },
    Command {
        input: Some(Input { kind: "text", prompt: "Node" }),
        ..command(ROAM_BUFFER_DEDICATED, "Roam: show the org-roam buffer of a node")
    },
    command(ROAM_BUFFER_REFRESH, "Roam: refresh the org-roam buffer"),
    Command { input: Some(Input { kind: "text", prompt: "Alias" }), ..edit(ROAM_ALIAS_ADD, "Roam: add an alias") },
    Command {
        input: Some(Input { kind: "text", prompt: "Remove" }),
        ..edit(ROAM_ALIAS_REMOVE, "Roam: remove an alias")
    },
    Command { input: Some(Input { kind: "text", prompt: "Tag" }), ..edit(ROAM_TAG_ADD, "Roam: add tags") },
    Command { input: Some(Input { kind: "text", prompt: "Tag" }), ..edit(ROAM_TAG_REMOVE, "Roam: remove tags") },
    Command { input: Some(Input { kind: "text", prompt: "Ref" }), ..edit(ROAM_REF_ADD, "Roam: add a reference") },
    Command {
        input: Some(Input { kind: "text", prompt: "Remove" }),
        ..edit(ROAM_REF_REMOVE, "Roam: remove a reference")
    },
    // org-journal's keys, under the prefix its README suggests (`C-c j `)
    // rather than its default (`C-c C-`), whose keys are Org's.
    Command {
        emacs: &[key(Global, "C-c j j")],
        vim: &[key(Global, "<Leader>jj")],
        ..command(JOURNAL_NEW_ENTRY, "Journal: new entry")
    },
    Command {
        input: Some(Input { kind: "date", prompt: "Date" }),
        ..command(JOURNAL_NEW_DATE_ENTRY, "Journal: new entry for a date")
    },
    Command {
        input: Some(Input { kind: "date", prompt: "Date" }),
        ..command(JOURNAL_NEW_SCHEDULED_ENTRY, "Journal: new entry for a day to come")
    },
    Command {
        emacs: &[key(Org, "C-c j f")],
        vim: &[key(Org, "<Leader>jf")],
        ..command(JOURNAL_NEXT_ENTRY, "Journal: next entry")
    },
    Command {
        emacs: &[key(Org, "C-c j b")],
        vim: &[key(Org, "<Leader>jb")],
        ..command(JOURNAL_PREVIOUS_ENTRY, "Journal: previous entry")
    },
    command(JOURNAL_OPEN_CURRENT, "Journal: open the current file"),
    // The search asks for the days to search too, unless with `C-u`.
    Command {
        emacs: &[key(Org, "C-c j s")],
        vim: &[key(Org, "<Leader>js")],
        input: Some(Input { kind: "text", prompt: "Enter a string to search for" }),
        ..command(JOURNAL_SEARCH, "Journal: search between two dates")
    },
    Command {
        input: Some(Input { kind: "text", prompt: "Enter a string to search for" }),
        ..command(JOURNAL_SEARCH_FOREVER, "Journal: search every entry")
    },
    Command {
        input: Some(Input { kind: "text", prompt: "Enter a string to search for" }),
        ..command(JOURNAL_SEARCH_FUTURE, "Journal: search the entries to come")
    },
    command(JOURNAL_SEARCH_FUTURE_SCHEDULED, "Journal: the TODO entries to come"),
    command(JOURNAL_SCHEDULE_VIEW, "Journal: the entries to come with a date"),
    Command {
        input: Some(Input { kind: "date", prompt: "Date" }),
        ..command(JOURNAL_RESCHEDULE, "Journal: move the entry to come to another day")
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
    // `C-u C-c C-s` and `C-u C-c C-d`: the date taken out; on an agenda
    // line, of its entry.
    Command { in_agenda: true, ..edit(SCHEDULE_REMOVE, "Remove the scheduled date of the entry") },
    Command { in_agenda: true, ..edit(DEADLINE_REMOVE, "Remove the deadline of the entry") },
    // `C-u C-u C-c C-s` and `C-u C-u C-c C-d`: a delay for the scheduled
    // date, a warning period for the deadline, to the day asked for.
    Command { in_agenda: true, ..ask_date(SCHEDULE_DELAY, "Delay the scheduled date until a day", "Delay until") },
    Command { in_agenda: true, ..ask_date(DEADLINE_WARNING, "Warn of the deadline from a day", "Warn starting from") },
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
/// The commands that read the editor's closed folds: their context has
/// them in `folded`, as the first and last line of each. Exporting the
/// visible part only (`C-v`) leaves them out.
pub const FOLDS: &[&str] = &[EXPORT];

/// The commands that work on a selection, Emacs's active region, when
/// they are run on one: editors with a visual mode give them their keys
/// there too.
pub const REGION: &[&str] = &[
    META_LEFT,
    META_RIGHT,
    META_UP,
    META_DOWN,
    SHIFT_META_LEFT,
    SHIFT_META_RIGHT,
    CHECKBOX_TOGGLE,
    TABLE_CREATE,
    // Off a table, `C-c -` and `C-c *` (`<Leader>o*`) turn lines into
    // items and headings.
    TABLE_HLINE,
    TABLE_RECALCULATE,
    TABLE_RECALCULATE_LINE,
    TOGGLE_ITEM,
    TOGGLE_HEADING,
    EMPHASIZE,
    STRUCTURE_TEMPLATE,
    DRAWER_INSERT,
    // Its text the search of the link (`org-link-precise-link-target`).
    LINK_STORE,
    // Its text the link's description, the link in its place.
    LINK_INSERT,
    ROAM_NODE_INSERT,
    // The subtrees in it refiled together.
    REFILE,
    REFILE_COPY,
    // In a table, the cells in it (`org-table-cut-region`,
    // `org-table-copy-region`).
    CUT_SUBTREE,
    COPY_SUBTREE,
    // The timer values in it shifted.
    TIMER_START,
];

/// `{ "command", "title", "edits", "keys": [{ "key", "in", "preset" }] }`.
/// A key that keeps its own meaning where the command does not apply has
/// `"native": true` as well. A command that asks for something has
/// `"input": { "kind", "prompt" }`; one that reads the clipboard,
/// `"clipboard": true`; one that works on a selection, `"region": true`
/// (see [`REGION`]); one that reads the editor's closed folds,
/// `"folds": true` (see [`FOLDS`]).
/// The agenda's Emacs keys the vim preset leaves to Vim: `$` is the
/// motion to the end of the line there, not `org-agenda-archive`.
const VIM_AGENDA_MOTIONS: &[&str] = &["$"];

pub fn manifest() -> Value {
    let commands: Vec<Value> = COMMANDS
        .iter()
        .map(|command| {
            // The agenda's keys, in Vim's notation for the vim preset.
            let agenda: Vec<(String, &Key)> = command
                .emacs
                .iter()
                .filter(|key| key.scope == Scope::Agenda && !VIM_AGENDA_MOTIONS.contains(&key.key))
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
            if REGION.contains(&command.id) {
                entry["region"] = json!(true);
            }
            if FOLDS.contains(&command.id) {
                entry["folds"] = json!(true);
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

/// The command Emacs's `key` runs in the first of `scopes` that has it
/// (`C-c C-c` in a capture buffer files the entry, in an Org file it
/// toggles a checkbox).
pub fn by_key(key: &str, scopes: &[Scope]) -> Option<&'static str> {
    scopes.iter().find_map(|scope| {
        COMMANDS
            .iter()
            .find(|command| command.emacs.iter().any(|bound| bound.scope == *scope && bound.key == key))
            .map(|command| command.id)
    })
}

/// The commands of the agenda, in the table's order: those with a key
/// there, and those that work on an agenda line.
pub fn in_agenda() -> impl Iterator<Item = &'static str> {
    COMMANDS
        .iter()
        .filter(|command| command.in_agenda || command.emacs.iter().any(|key| key.scope == Scope::Agenda))
        .map(|command| command.id)
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
        // The same keys but those left to Vim's motions, and with the vim
        // preset the leader keys of Org files besides.
        let vim: Vec<(String, String)> =
            keys("vim").into_iter().filter(|(_, key)| !key.starts_with("<Leader>")).collect();
        assert_eq!(emacs.len() - VIM_AGENDA_MOTIONS.len(), vim.len());
        assert!(!vim.iter().any(|(_, key)| key == "$"));
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

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn finds_what_a_key_runs_where_it_is_pressed() {
        assert_eq!(by_key("C-c C-c", &[Scope::Capture, Scope::Org]), Some(CAPTURE_FINISH));
        assert_eq!(by_key("C-c C-c", &[Scope::Org]), Some("org.checkbox.toggle"));
        assert_eq!(by_key("C-c C-w", &[Scope::Capture, Scope::Org]), Some(CAPTURE_REFILE));
        assert_eq!(by_key("C-c C-t", &[Scope::Capture, Scope::Org]), Some("org.todo"));
        assert_eq!(by_key("f", &[Scope::Agenda]), Some("org.agenda.later"));
        assert_eq!(by_key("C-c C-y C-z", &[Scope::Org]), None);
    }
}
