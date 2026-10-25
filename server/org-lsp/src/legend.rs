//! What the semantic tokens mean and how they look by default.
//!
//! Editors style tokens through their own themes, so each needs a mapping
//! from token types and modifiers to its highlight names. Those mappings
//! are kept here, next to the tokens, and announced with the rest of the
//! manifest: an editor integration applies the table instead of carrying
//! one.

use serde_json::{json, Value};

use crate::highlight::{modifier, TokenType};

/// Token types that LSP defines itself; editors already know them.
const STANDARD_TYPES: &[TokenType] = &[TokenType::Property, TokenType::Keyword, TokenType::Comment, TokenType::Macro];

fn describe_type(ty: TokenType) -> &'static str {
    match ty {
        TokenType::Headline => "The title text of a headline",
        TokenType::Stars => "The stars that start a headline",
        TokenType::TodoKeyword => "A TODO keyword",
        TokenType::Priority => "A priority cookie such as [#A]",
        TokenType::Tag => "Headline tags",
        TokenType::Planning => "SCHEDULED:, DEADLINE:, CLOSED: and CLOCK: keywords",
        TokenType::Timestamp => "A timestamp",
        TokenType::Drawer => "The opening or closing line of a drawer",
        TokenType::Property => "The key of a property",
        TokenType::Keyword => "The key of a #+KEYWORD: line",
        TokenType::Block => "The opening or closing line of a block",
        TokenType::Comment => "A comment",
        TokenType::Bullet => "A list bullet",
        TokenType::Checkbox => "A list checkbox",
        TokenType::Link => "The brackets of a link, and its target when it has a description and Org hides it; a radio link; a citation",
        TokenType::Code => "Inline code",
        TokenType::Verbatim => "Verbatim text",
        TokenType::Text => "Text that carries emphasis",
        TokenType::TableDelimiter => "Table borders and rules",
        TokenType::Footnote => "A footnote reference or label",
        TokenType::Cookie => "A statistics cookie such as [1/3]",
        TokenType::Target => "A link target",
        TokenType::Latex => "A LaTeX fragment",
        TokenType::Entity => "An entity such as \\alpha",
        TokenType::Macro => "A macro call",
        TokenType::DocumentTitle => "The value of #+TITLE:",
        TokenType::DocumentInfo => "The value of #+SUBTITLE:, #+AUTHOR:, #+EMAIL: or #+DATE:",
        TokenType::AgendaStructure => "Agenda: the title line of a view",
        TokenType::AgendaDate => "Agenda: a date line",
        TokenType::AgendaScheduled => "Agenda: a scheduled entry",
        TokenType::AgendaDeadline => "Agenda: an entry with a deadline",
        TokenType::AgendaEvent => "Agenda: an entry with a plain timestamp",
        TokenType::AgendaDone => "Agenda: an entry that is done",
        TokenType::AgendaDimmed => "Agenda: a task its dependencies block, dimmed",
        TokenType::AgendaHabit => "Agenda: a day of a habit's consistency graph, before it is due",
        TokenType::AgendaTimeGrid => "Agenda: a line of the time grid",
        TokenType::AgendaClocking => "Agenda: the line of the entry the clock runs on, between its other parts",
        TokenType::CalendarHeader => "Date prompt: the month and year above a month",
        TokenType::CalendarWeekday => "Date prompt: the name of a weekday above its column",
        TokenType::CalendarSelected => "Date prompt: the selected day",
        TokenType::CalendarPrompt => "Date prompt: the prompt before the answer",
        TokenType::CalendarJournal => "Date prompt: a day with an entry of org-journal; to come with `future`",
        TokenType::CalendarDailies => "Date prompt: a day with a daily note of org-roam",
        TokenType::Quote => "The text of a quote block, with org_fontify_quote_and_verse_blocks",
        TokenType::Verse => "The text of a verse block, with org_fontify_quote_and_verse_blocks",
        TokenType::Outline => {
            "A headline or its stars in the color of its level, as Emacs's outline-N faces; never sent by the server: VS Code's org-lsp.headlineLevelColors turns headline and stars tokens into it"
        }
        TokenType::DescriptionTerm => "The term of a description list item, with its ::",
        TokenType::LinkDescription => "The description of a link, [[target][description]]",
        TokenType::LinkUrl => "The target of a link: a link without a description, a plain or angle link, or the target of a described one Org shows",
        TokenType::LatexEnvironment => "A LaTeX environment, \\begin{…} to \\end{…}",
        TokenType::TableHeader => "The cells of a table's header: the rows above its first rule, when rows follow it",
    }
}

/// Descriptions of the modifiers, in the order of `modifier::NAMES`.
const MODIFIER_DESCRIPTIONS: &[&str] = &[
    "Headline level 1",
    "Headline level 2",
    "Headline level 3",
    "Headline level 4",
    "Headline level 5",
    "Headline level 6",
    "Headline level 7",
    "Headline level 8",
    "A TODO keyword that marks the entry as finished; the rest of its headline; a complete statistics cookie",
    "An inactive timestamp",
    "A ticked checkbox",
    "A partially ticked checkbox",
    "Bold text",
    "Italic text",
    "Underlined text",
    "Struck-through text",
    "Agenda: today",
    "Agenda: a weekend day",
    "Agenda: scheduled before today",
    "Agenda: the deadline is today or has passed",
    "Agenda: the deadline is far away",
    "Agenda: a day inside a date range",
    "Agenda: the current time",
    "Text Org hides (link brackets and targets, emphasis markers, hidden keywords, macro braces); editors that can conceal text conceal it",
    "Agenda: part of the line of the entry the clock runs on",
    "A headline with the ARCHIVE tag",
    "Agenda: a day of a habit's graph in its time to be done",
    "Agenda: the day a habit is due by",
    "Agenda: a day of a habit's graph past its due day",
    "Agenda: a day of a habit's graph to come",
    "A headline marked with a TODO keyword not done, past the keyword, with org_fontify_todo_headline",
    "The stars and title of an inline task",
];

/// The default look of the tokens a selector matches.
pub struct Look {
    /// `type`, `type.modifier` or `*.modifier`; or, for a look with a
    /// background only, `type.modifier.modifier`: tokens with both.
    pub selector: &'static str,
    /// Vim highlight groups: the first one that is defined is used. The
    /// `@org.*` names are the groups nvim-orgmode colorschemes define.
    pub vim: &'static [&'static str],
    /// TextMate scopes, most specific first, as themes for VS Code and
    /// similar editors match them: the first one a theme has a rule for
    /// is used, so one that themes color apart comes before one that a
    /// broad rule (`keyword`, `markup.heading`) would take for another
    /// look as well.
    pub textmate: &'static [&'static str],
    /// For a look that is a background only, the background of Org's
    /// face on a light and on a dark background: what an editor shows
    /// when its theme has none of the groups above.
    pub background: Option<(&'static str, &'static str)>,
}

const fn look(selector: &'static str, vim: &'static [&'static str], textmate: &'static [&'static str]) -> Look {
    Look { selector, vim, textmate, background: None }
}

/// A look with the background of Org's face, `(light, dark)`.
const fn look_on(
    selector: &'static str,
    vim: &'static [&'static str],
    textmate: &'static [&'static str],
    background: (&'static str, &'static str),
) -> Look {
    Look { selector, vim, textmate, background: Some(background) }
}

pub const LOOKS: &[Look] = &[
    look("headline", &[], &["markup.heading.org", "markup.heading", "entity.name.section"]),
    look("headline.level1", &["@org.headline.level1", "Title"], &["markup.heading.1.org", "entity.name.function"]),
    look("headline.level2", &["@org.headline.level2", "Constant"], &["markup.heading.2.org", "variable.parameter"]),
    look("headline.level3", &["@org.headline.level3", "Identifier"], &["markup.heading.3.org", "keyword"]),
    look("headline.level4", &["@org.headline.level4", "Statement"], &["markup.heading.4.org", "comment"]),
    look("headline.level5", &["@org.headline.level5", "PreProc"], &["markup.heading.5.org", "storage.type"]),
    look("headline.level6", &["@org.headline.level6", "Type"], &["markup.heading.6.org", "constant.language"]),
    look("headline.level7", &["@org.headline.level7", "Special"], &["markup.heading.7.org", "support.function"]),
    look("headline.level8", &["@org.headline.level8", "String"], &["markup.heading.8.org", "string"]),
    // The faces `org-level-N` inherit, `outline-1` to `outline-8`, in the
    // theme's colors of code, without `markup.heading`, which themes give
    // every level alike: what VS Code shows with
    // `org-lsp.headlineLevelColors` (the server never sends `outline`).
    // Visual Studio Dark and Light color no function names, parameters or
    // `support.function`, and Solarized no parameters: a macro's name, the
    // language's own variables and built-in property names stand in.
    look("outline", &[], &[]),
    look(
        "outline.level1",
        &[],
        &["entity.name.function.outline-1.org", "entity.name.function", "entity.name.function.preprocessor"],
    ),
    look("outline.level2", &[], &["variable.parameter.outline-2.org", "variable.parameter", "variable.language"]),
    look("outline.level3", &[], &["keyword.outline-3.org", "keyword"]),
    look("outline.level4", &[], &["comment.outline-4.org", "comment"]),
    look("outline.level5", &[], &["storage.type.outline-5.org", "storage.type"]),
    look("outline.level6", &[], &["constant.language.outline-6.org", "constant.language"]),
    look("outline.level7", &[], &["support.function.outline-7.org", "support.function", "support.type.property-name"]),
    look("outline.level8", &[], &["string.outline-8.org", "string"]),
    look("stars", &[], &["punctuation.definition.heading.org", "markup.heading"]),
    look("stars.level1", &["@org.headline.level1", "Title"], &[]),
    look("stars.level2", &["@org.headline.level2", "Constant"], &[]),
    look("stars.level3", &["@org.headline.level3", "Identifier"], &[]),
    look("stars.level4", &["@org.headline.level4", "Statement"], &[]),
    look("stars.level5", &["@org.headline.level5", "PreProc"], &[]),
    look("stars.level6", &["@org.headline.level6", "Type"], &[]),
    look("stars.level7", &["@org.headline.level7", "Special"], &[]),
    look("stars.level8", &["@org.headline.level8", "String"], &[]),
    look(
        "todoKeyword",
        &["@org.keyword.todo", "DiagnosticError"],
        &["markup.deleted.todo.org", "keyword.other.todo.org", "keyword.control"],
    ),
    look(
        "todoKeyword.done",
        &["@org.keyword.done", "DiagnosticOk"],
        &["markup.inserted.done.org", "keyword.other.done.org", "string"],
    ),
    look(
        "priority",
        &["@org.priority.highest", "@comment.error"],
        &["constant.other.priority.org", "constant.numeric"],
    ),
    look("tag", &["@org.tag", "@tag.attribute"], &["entity.name.tag.org", "entity.name.tag"]),
    look("planning", &["@org.plan", "Constant"], &["keyword.other.planning.org", "keyword.other"]),
    look("timestamp", &["@org.timestamp.active", "@keyword"], &["constant.other.timestamp.org", "constant.numeric"]),
    look("timestamp.inactive", &["@org.timestamp.inactive", "@comment"], &["comment.timestamp.org", "comment"]),
    look(
        "drawer",
        &["@org.drawer", "@property"],
        &["meta.preprocessor.drawer.org", "meta.preprocessor", "support.function"],
    ),
    look(
        "property",
        &["@org.properties.name", "@property"],
        &["variable.other.property.org", "variable.other.property", "entity.other.attribute-name"],
    ),
    look(
        "keyword",
        &["@org.directive", "@comment"],
        &["meta.preprocessor.keyword.org", "meta.preprocessor", "comment"],
    ),
    look("block", &["@org.block", "@comment"], &["punctuation.definition.block.org", "comment"]),
    // `org-quote` and `org-verse`, which are `org-block`. Dark+, Light+
    // and the Modern themes give `markup.quote` no color, but italics.
    look("quote", &["@org.quote", "@markup.quote"], &["markup.quote.org", "markup.quote", "markup.italic"]),
    look("verse", &["@org.verse", "@markup.quote"], &["markup.quote.verse.org", "markup.quote", "markup.italic"]),
    look("comment", &["@org.comment", "@comment"], &["comment.line.org", "comment"]),
    look(
        "bullet",
        &["@org.bullet", "@markup.list"],
        &["punctuation.definition.list.begin.org", "punctuation.definition.list.begin.markdown", "keyword.operator"],
    ),
    look("checkbox", &["@org.checkbox", "@markup.list.unchecked"], &["markup.list.unchecked.org", "keyword.operator"]),
    look(
        "checkbox.checked",
        &["@org.checkbox.checked", "@markup.list.checked"],
        &["markup.list.checked.org", "markup.inserted", "string"],
    ),
    look("checkbox.partial", &["@org.checkbox.halfchecked", "@markup.list.unchecked"], &[]),
    // `org-list-dt`: bold, as bold emphasis is.
    look("descriptionTerm", &["@org.bold", "@markup.strong"], &["markup.bold.term.org", "markup.bold"]),
    look(
        "link",
        &["@org.hyperlink", "@markup.link"],
        &["markup.underline.link.org", "markup.underline.link", "string.other.link"],
    ),
    // The parts of a link as nvim-orgmode colors them.
    look(
        "linkDescription",
        &["@org.hyperlink.desc", "@markup.link.label"],
        &[
            "markup.underline.link.description.org",
            "markup.underline.link",
            "string.other.link.description",
            "string.other.link",
        ],
    ),
    look(
        "linkUrl",
        &["@org.hyperlink.url", "@markup.link.url"],
        &["markup.underline.link.url.org", "markup.underline.link", "string.other.link"],
    ),
    look("code", &["@org.code", "@markup.raw"], &["markup.inline.raw.org", "markup.inline.raw", "string"]),
    look("verbatim", &["@org.verbatim", "@markup.raw"], &["markup.inline.raw.org", "markup.inline.raw", "string"]),
    look("text", &[], &[]),
    look(
        "tableDelimiter",
        &["@org.table.delimiter", "@punctuation.special"],
        &["punctuation.separator.table.org", "punctuation.definition.table", "comment"],
    ),
    // The header of a table: Org colors it only in the header line of
    // `org-table-header-line-mode` (`org-table-header`); org-lsp in the
    // table, as nvim-orgmode does.
    look(
        "tableHeader",
        &["@org.table.heading", "@markup.heading"],
        &["markup.heading.table.org", "markup.heading", "entity.name.section"],
    ),
    look(
        "footnote",
        &["@org.footnote", "@markup.link.url"],
        &["markup.underline.link.footnote.org", "markup.underline.link", "string.other.link"],
    ),
    look("cookie", &["@org.priority.highest", "@number"], &["constant.numeric.cookie.org", "constant.numeric"]),
    // `org-checkbox-statistics-done`, which inherits `org-done`.
    look(
        "cookie.done",
        &["@org.keyword.done", "DiagnosticOk"],
        &["constant.numeric.cookie.done.org", "markup.inserted", "string"],
    ),
    // `org-headline-done`: what follows a done keyword on its headline.
    look(
        "headline.done",
        &["@org.headline.done", "Comment"],
        &["comment.headline.done.org", "markup.heading.done.org"],
    ),
    look("*.done", &["@org.headline.done", "Comment"], &["comment.done.org", "comment"]),
    // `org-headline-todo`, copied from `font-lock-string-face`.
    look("headline.todo", &["@org.headline.todo", "String"], &["string.headline.todo.org", "markup.heading.todo.org"]),
    look("*.todo", &["@org.headline.todo", "String"], &["string.todo.org", "string"]),
    look("documentTitle", &["@org.document.title", "Title"], &["markup.heading.title.org", "entity.name.section"]),
    look("documentInfo", &["@org.document.info", "Special"], &["string.other.info.org", "string"]),
    // `org-target`: underlined. Themes without a color for labels give
    // it a link's underline or color.
    look(
        "target",
        &["@org.hyperlink.desc", "@markup.link.label"],
        &["entity.name.label.org", "entity.name.label", "markup.underline.link", "string.other.link"],
    ),
    // `org-latex-and-related`, burlywood or saddle brown: TeX's math is
    // `string.other.math`, a string where a theme has no other color.
    look("latex", &["@org.latex", "@markup.math"], &["markup.math.org", "support.function", "string.other.math"]),
    look(
        "latexEnvironment",
        &["@org.latex_env", "@markup.environment"],
        &["markup.math.environment.org", "support.function", "string.other.math"],
    ),
    look(
        "entity",
        &["@org.latex", "@markup.math"],
        &["constant.character.entity.org", "constant.character.entity", "string.other.math"],
    ),
    look(
        "macro",
        &["@org.directive", "@function.macro"],
        &["entity.name.function.macro.org", "entity.name.function.preprocessor"],
    ),
    look("*.bold", &["@org.bold", "@markup.strong"], &["markup.bold.org", "markup.bold"]),
    look("*.italic", &["@org.italic", "@markup.italic"], &["markup.italic.org", "markup.italic"]),
    // A theme cannot underline through a fallback scope: without
    // `markup.underline`, the color of links, the text Emacs underlines.
    look(
        "*.underline",
        &["@org.underline", "@markup.underline"],
        &["markup.underline.org", "markup.underline", "string.other.link"],
    ),
    look(
        "*.strikethrough",
        &["@org.strikethrough", "@markup.strikethrough"],
        &["markup.strikethrough.org", "markup.strikethrough"],
    ),
    // The agenda. Where Org's own face adds nothing to the default one
    // (plain events, days of a date range), no look is given.
    look(
        "agendaStructure",
        &["@org.agenda.header", "Title"],
        &["markup.heading.agenda.org", "markup.heading", "entity.name.section"],
    ),
    look(
        "agendaDate",
        &["@org.agenda.day", "Statement"],
        &["markup.heading.date.agenda.org", "markup.heading", "entity.name.section"],
    ),
    look(
        "agendaDate.today",
        &["@org.agenda.today", "@markup.strong"],
        &["markup.bold.today.agenda.org", "markup.bold", "markup.heading", "entity.name.section"],
    ),
    look(
        "agendaDate.weekend",
        &["@org.agenda.weekend", "@markup.italic"],
        &["markup.italic.weekend.agenda.org", "markup.italic", "markup.heading", "entity.name.section"],
    ),
    look(
        "agendaScheduled",
        &["@org.agenda.scheduled", "DiagnosticOk"],
        &["markup.inserted.scheduled.agenda.org", "markup.inserted", "string"],
    ),
    look(
        "agendaScheduled.past",
        &["@org.agenda.scheduled_past", "DiagnosticWarn"],
        &["markup.changed.scheduled.agenda.org", "markup.changed", "constant.numeric"],
    ),
    look(
        "agendaDeadline",
        &["@org.agenda.deadline.upcoming", "DiagnosticWarn"],
        &["markup.changed.deadline.agenda.org", "markup.changed", "constant.numeric"],
    ),
    look(
        "agendaDeadline.imminent",
        &["@org.agenda.deadline", "DiagnosticError"],
        &["markup.deleted.deadline.agenda.org", "markup.deleted", "keyword.control"],
    ),
    look(
        "agendaDeadline.distant",
        &["@org.agenda.deadline.distant", "Comment"],
        &["comment.deadline.agenda.org", "comment"],
    ),
    look("agendaEvent", &[], &[]),
    look("agendaDone", &["@org.agenda.done", "Comment"], &["comment.done.agenda.org", "comment"]),
    // A habit's graph: Org colors the days' backgrounds, blue before a
    // habit is due (`org-habit-clear-face`), green while it is
    // (`org-habit-ready-face`), yellow on the day it must be done by
    // (`org-habit-alert-face`) and red after (`org-habit-overdue-face`).
    // No standard group has those backgrounds (Neovim's `DiffDelete` has
    // none at all), so the editors take Org's colors. Tomorrow Night Blue
    // colors insertions, changes and deletions only in the gutter
    // (`*.git_gutter`).
    look_on(
        "agendaHabit",
        &["@org.agenda.habit.clear"],
        &["markup.changed.habit.agenda.org", "markup.changed", "markup.changed.git_gutter"],
        ("#8270f9", "#0000ff"),
    ),
    look_on(
        "agendaHabit.ready",
        &["@org.agenda.habit.ready"],
        &["markup.inserted.habit.agenda.org", "markup.inserted", "markup.inserted.git_gutter"],
        ("#4df946", "#228b22"),
    ),
    look_on(
        "agendaHabit.alert",
        &["@org.agenda.habit.alert"],
        &["markup.changed.alert.habit.agenda.org", "markup.changed", "constant.numeric"],
        ("#f5f946", "#ffd700"),
    ),
    look_on(
        "agendaHabit.overdue",
        &["@org.agenda.habit.overdue"],
        &["markup.deleted.habit.agenda.org", "markup.deleted", "markup.deleted.git_gutter"],
        ("#f9372d", "#b22222"),
    ),
    // The `-future` faces, paler: a token has `future` with the others.
    look_on("agendaHabit.future", &[], &[], ("#d6e4fc", "#191970")),
    look_on("agendaHabit.ready.future", &[], &[], ("#acfca9", "#006400")),
    look_on("agendaHabit.alert.future", &[], &[], ("#fafca9", "#b8860b")),
    look_on("agendaHabit.overdue.future", &[], &[], ("#fc9590", "#8b0000")),
    look("agendaTimeGrid", &["@org.agenda.time_grid", "Comment"], &["comment.time-grid.agenda.org", "comment"]),
    look(
        "agendaTimeGrid.current",
        &["@org.agenda.time_grid.current", "@markup.strong"],
        &["markup.bold.current-time.agenda.org", "markup.bold", "comment"],
    ),
    // `org-agenda-clocking`: the line of the clocked entry, highlighted,
    // with the background of `secondary-selection` it inherits for an
    // editor that has no group or line highlight for it (Zed).
    look_on(
        "agendaClocking",
        &["@org.agenda.clocking", "Visual"],
        &["markup.highlight.clocking.agenda.org", "markup.inserted", "markup.inserted.git_gutter"],
        ("#ffff00", "#4a708b"),
    ),
    look_on(
        "*.clocking",
        &["@org.agenda.clocking", "Visual"],
        &["markup.highlight.clocking.agenda.org", "markup.inserted", "markup.inserted.git_gutter"],
        ("#ffff00", "#4a708b"),
    ),
    // `org-agenda-dimmed-todo-face`: a task its dependencies block.
    look("agendaDimmed", &["@org.agenda.dimmed", "Comment"], &["comment.dimmed.agenda.org", "comment"]),
    // `org-archived`, which inherits `shadow`.
    look("*.archived", &["@org.archived", "Comment"], &["comment.archived.org", "comment"]),
    // `org-inlinetask`, which inherits `shadow`.
    look("*.inlinetask", &["@org.inlinetask", "Comment"], &["comment.inlinetask.org", "comment"]),
    // The date prompt, after Emacs's calendar and minibuffer faces.
    look(
        "calendarHeader",
        &["Function"],
        &["entity.name.function.month.calendar.org", "entity.name.function", "entity.name.function.preprocessor"],
    ),
    // `calendar-weekday-header`, `font-lock-constant-face`: Dark+, Light+
    // and the Modern themes color `constant.language`, not `constant.other`.
    look(
        "calendarWeekday",
        &["Constant"],
        &["constant.other.weekday.calendar.org", "constant.other", "constant.language"],
    ),
    look("calendarWeekday.weekend", &["Comment"], &["comment.weekend.calendar.org", "comment"]),
    look("calendarSelected", &["IncSearch"], &["markup.inverse.selected.calendar.org", "invalid.illegal"]),
    look("calendarPrompt", &["Question"], &["keyword.other.prompt.calendar.org", "keyword"]),
    // `org-journal-calendar-entry-face` and
    // `org-journal-calendar-scheduled-face`: red and italic.
    look(
        "calendarJournal",
        &["@org.calendar.journal", "Special"],
        &["markup.italic.journal.calendar.org", "markup.italic"],
    ),
    look(
        "calendarJournal.future",
        &["@org.calendar.journal.future", "Comment"],
        &["markup.italic.scheduled.journal.calendar.org", "markup.italic"],
    ),
    // `org-roam-dailies-calendar-note`: `org-link` without its underline.
    look(
        "calendarDailies",
        &["@org.calendar.dailies", "@markup.link"],
        &["string.other.link.dailies.calendar.org", "string.other.link"],
    ),
];

/// The legend with descriptions, and the default looks:
/// `{ "tokenTypes": [{ "id", "description", "standard" }],
///    "tokenModifiers": [{ "id", "description" }],
///    "highlights": [{ "token", "vim": [...], "textmate": [...],
///                     "background"?: { "light", "dark" } }] }`.
pub fn manifest() -> Value {
    let types: Vec<Value> = TokenType::ALL
        .iter()
        .map(
            |ty| json!({ "id": ty.name(), "description": describe_type(*ty), "standard": STANDARD_TYPES.contains(ty) }),
        )
        .collect();
    let modifiers: Vec<Value> = modifier::NAMES
        .iter()
        .zip(MODIFIER_DESCRIPTIONS)
        .map(|(name, description)| json!({ "id": name, "description": description }))
        .collect();
    let highlights: Vec<Value> = LOOKS
        .iter()
        .filter(|look| !look.vim.is_empty() || !look.textmate.is_empty() || look.background.is_some())
        .map(|look| {
            let mut entry = json!({ "token": look.selector, "vim": look.vim, "textmate": look.textmate });
            if let Some((light, dark)) = look.background {
                entry["background"] = json!({ "light": light, "dark": dark });
            }
            entry
        })
        .collect();
    json!({ "tokenTypes": types, "tokenModifiers": modifiers, "highlights": highlights })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_modifier_is_described() {
        assert_eq!(MODIFIER_DESCRIPTIONS.len(), modifier::NAMES.len());
    }

    #[test]
    fn looks_name_real_tokens_once() {
        let types: Vec<&str> = TokenType::ALL.iter().map(|ty| ty.name()).collect();
        let mut seen = Vec::new();
        for look in LOOKS {
            let mut parts = look.selector.split('.');
            let ty = parts.next().unwrap_or_default();
            let modifiers: Vec<&str> = parts.collect();
            assert!(ty == "*" || types.contains(&ty), "unknown type in `{}`", look.selector);
            assert!(
                modifiers.iter().all(|name| modifier::NAMES.contains(name)),
                "unknown modifier in `{}`",
                look.selector
            );
            assert!(
                modifiers.len() < 2 || (ty != "*" && look.vim.is_empty() && look.textmate.is_empty()),
                "`{}`: only a background can be given to several modifiers",
                look.selector
            );
            assert!(!seen.contains(&look.selector), "`{}` is listed twice", look.selector);
            seen.push(look.selector);
        }
    }

    #[test]
    fn every_type_has_a_look_or_says_it_has_none() {
        for ty in TokenType::ALL {
            assert!(LOOKS.iter().any(|look| look.selector == ty.name()), "`{}` has no entry in LOOKS", ty.name());
        }
    }
}
