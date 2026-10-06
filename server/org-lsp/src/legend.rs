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
        TokenType::Link => "A link",
        TokenType::Code => "Inline code",
        TokenType::Verbatim => "Verbatim text",
        TokenType::Text => "Text that carries emphasis",
        TokenType::TableDelimiter => "Table borders and rules",
        TokenType::Footnote => "A footnote reference or label",
        TokenType::Cookie => "A statistics cookie such as [1/3]",
        TokenType::Target => "A link target",
        TokenType::Latex => "A LaTeX fragment or environment",
        TokenType::Entity => "An entity such as \\alpha",
        TokenType::Macro => "A macro call",
        TokenType::AgendaStructure => "Agenda: the title line of a view",
        TokenType::AgendaDate => "Agenda: a date line",
        TokenType::AgendaScheduled => "Agenda: a scheduled entry",
        TokenType::AgendaDeadline => "Agenda: an entry with a deadline",
        TokenType::AgendaEvent => "Agenda: an entry with a plain timestamp",
        TokenType::AgendaDone => "Agenda: an entry that is done",
        TokenType::AgendaTimeGrid => "Agenda: a line of the time grid",
        TokenType::AgendaClocking => "Agenda: the line of the entry the clock runs on, between its other parts",
        TokenType::CalendarHeader => "Date prompt: the month and year above a month",
        TokenType::CalendarWeekday => "Date prompt: the name of a weekday above its column",
        TokenType::CalendarSelected => "Date prompt: the selected day",
        TokenType::CalendarPrompt => "Date prompt: the prompt before the answer",
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
    "A TODO keyword that marks the entry as finished",
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
    "Text Org hides (link brackets and targets, emphasis markers); editors that can conceal text conceal it",
    "Agenda: part of the line of the entry the clock runs on",
    "A headline with the ARCHIVE tag",
];

/// The default look of the tokens a selector matches.
pub struct Look {
    /// `type`, `type.modifier` or `*.modifier`.
    pub selector: &'static str,
    /// Vim highlight groups: the first one that is defined is used. The
    /// `@org.*` names are the groups nvim-orgmode colorschemes define.
    pub vim: &'static [&'static str],
    /// TextMate scopes, most specific first, as themes for VS Code and
    /// similar editors match them.
    pub textmate: &'static [&'static str],
}

const fn look(selector: &'static str, vim: &'static [&'static str], textmate: &'static [&'static str]) -> Look {
    Look { selector, vim, textmate }
}

pub const LOOKS: &[Look] = &[
    look("headline", &[], &["markup.heading.org", "markup.heading", "entity.name.section"]),
    look("headline.level1", &["@org.headline.level1", "Title"], &["markup.heading.1.org"]),
    look("headline.level2", &["@org.headline.level2", "Constant"], &["markup.heading.2.org"]),
    look("headline.level3", &["@org.headline.level3", "Identifier"], &["markup.heading.3.org"]),
    look("headline.level4", &["@org.headline.level4", "Statement"], &["markup.heading.4.org"]),
    look("headline.level5", &["@org.headline.level5", "PreProc"], &["markup.heading.5.org"]),
    look("headline.level6", &["@org.headline.level6", "Type"], &["markup.heading.6.org"]),
    look("headline.level7", &["@org.headline.level7", "Special"], &["markup.heading.7.org"]),
    look("headline.level8", &["@org.headline.level8", "String"], &["markup.heading.8.org"]),
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
        &["keyword.other.todo.org", "markup.deleted", "keyword.control"],
    ),
    look(
        "todoKeyword.done",
        &["@org.keyword.done", "DiagnosticOk"],
        &["keyword.other.done.org", "markup.inserted", "string"],
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
    look("drawer", &["@org.drawer", "@property"], &["meta.preprocessor.drawer.org", "meta.preprocessor"]),
    look(
        "property",
        &["@org.properties.name", "@property"],
        &["variable.other.property.org", "variable.other.property"],
    ),
    look("keyword", &["@org.directive", "@comment"], &["meta.preprocessor.keyword.org", "meta.preprocessor"]),
    look("block", &["@org.block", "@comment"], &["punctuation.definition.block.org", "comment"]),
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
        &["markup.list.checked.org", "markup.inserted"],
    ),
    look("checkbox.partial", &["@org.checkbox.halfchecked", "@markup.list.unchecked"], &[]),
    look(
        "link",
        &["@org.hyperlink", "@markup.link"],
        &["markup.underline.link.org", "markup.underline.link", "string.other.link"],
    ),
    look("code", &["@org.code", "@markup.raw"], &["markup.inline.raw.org", "markup.inline.raw"]),
    look("verbatim", &["@org.verbatim", "@markup.raw"], &["markup.inline.raw.org", "markup.inline.raw"]),
    look("text", &[], &[]),
    look(
        "tableDelimiter",
        &["@org.table.delimiter", "@punctuation.special"],
        &["punctuation.separator.table.org", "punctuation.definition.table", "comment"],
    ),
    look(
        "footnote",
        &["@org.footnote", "@markup.link.url"],
        &["markup.underline.link.footnote.org", "markup.underline.link"],
    ),
    look("cookie", &["@org.priority.highest", "@number"], &["constant.numeric.cookie.org", "constant.numeric"]),
    look("target", &["@org.hyperlink.desc", "@markup.link.label"], &["entity.name.label.org", "entity.name.label"]),
    look("latex", &["@org.latex", "@markup.math"], &["markup.math.org", "support.function"]),
    look("entity", &["@org.latex", "@markup.math"], &["constant.character.entity.org", "constant.character.entity"]),
    look(
        "macro",
        &["@org.directive", "@function.macro"],
        &["entity.name.function.macro.org", "entity.name.function.preprocessor"],
    ),
    look("*.bold", &["@org.bold", "@markup.strong"], &["markup.bold.org", "markup.bold"]),
    look("*.italic", &["@org.italic", "@markup.italic"], &["markup.italic.org", "markup.italic"]),
    look("*.underline", &["@org.underline", "@markup.underline"], &["markup.underline.org", "markup.underline"]),
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
        &["markup.bold.today.agenda.org", "markup.bold"],
    ),
    look(
        "agendaDate.weekend",
        &["@org.agenda.weekend", "@markup.italic"],
        &["markup.italic.weekend.agenda.org", "markup.italic"],
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
    look("agendaTimeGrid", &["@org.agenda.time_grid", "Comment"], &["comment.time-grid.agenda.org", "comment"]),
    look(
        "agendaTimeGrid.current",
        &["@org.agenda.time_grid.current", "@markup.strong"],
        &["markup.bold.current-time.agenda.org", "markup.bold"],
    ),
    // `org-agenda-clocking`: the line of the clocked entry, highlighted.
    look(
        "agendaClocking",
        &["@org.agenda.clocking", "Visual"],
        &["markup.highlight.clocking.agenda.org", "markup.inserted"],
    ),
    look(
        "*.clocking",
        &["@org.agenda.clocking", "Visual"],
        &["markup.highlight.clocking.agenda.org", "markup.inserted"],
    ),
    // `org-archived`, which inherits `shadow`.
    look("*.archived", &["@org.archived", "Comment"], &["comment.archived.org", "comment"]),
    // The date prompt, after Emacs's calendar and minibuffer faces.
    look("calendarHeader", &["Function"], &["entity.name.function.month.calendar.org", "entity.name.function"]),
    look("calendarWeekday", &["Constant"], &["constant.other.weekday.calendar.org", "constant.other"]),
    look("calendarWeekday.weekend", &["Comment"], &["comment.weekend.calendar.org", "comment"]),
    look("calendarSelected", &["IncSearch"], &["markup.inverse.selected.calendar.org", "invalid.illegal"]),
    look("calendarPrompt", &["Question"], &["keyword.other.prompt.calendar.org", "keyword"]),
];

/// The legend with descriptions, and the default looks:
/// `{ "tokenTypes": [{ "id", "description", "standard" }],
///    "tokenModifiers": [{ "id", "description" }],
///    "highlights": [{ "token", "vim": [...], "textmate": [...] }] }`.
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
        .filter(|look| !look.vim.is_empty() || !look.textmate.is_empty())
        .map(|look| json!({ "token": look.selector, "vim": look.vim, "textmate": look.textmate }))
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
            let (ty, modifier) = match look.selector.split_once('.') {
                Some((ty, modifier)) => (ty, Some(modifier)),
                None => (look.selector, None),
            };
            assert!(ty == "*" || types.contains(&ty), "unknown type in `{}`", look.selector);
            assert!(
                modifier.is_none_or(|name| modifier::NAMES.contains(&name)),
                "unknown modifier in `{}`",
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
