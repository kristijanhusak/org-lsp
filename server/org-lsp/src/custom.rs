//! `org-agenda-custom-commands`: the commands by their keys, and the
//! blocks of the view each one shows.

use org_core::agenda::{Config, Matcher, Now};
use org_core::date::{DateTime, Day};
use org_core::read_date::{self, Settings as ReadDateSettings};

use crate::config::{AgendaOptions, CustomBlockOption, CustomCommandOption};
use crate::views::{Block, BlockKind, Moved};

/// A command that shows a view: it has a type or blocks. One with
/// neither is a prefix key for the commands after it.
pub fn is_view(command: &CustomCommandOption) -> bool {
    command.kind.is_some() || command.blocks.is_some()
}

/// Whether `command` shows a search view, alone or as a block.
pub fn has_search(command: &CustomCommandOption) -> bool {
    command.kind.as_deref() == Some("search")
        || command.blocks.iter().flatten().any(|block| block.kind.as_str() == "search")
}

/// The command with exactly these keys.
pub fn find<'a>(commands: &'a [CustomCommandOption], keys: &str) -> Option<&'a CustomCommandOption> {
    commands.iter().find(|command| command.key == keys && is_view(command))
}

/// The commands and prefixes one key after `prefix`: the key and how the
/// menu shows it.
pub fn next_keys(commands: &[CustomCommandOption], prefix: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for command in commands {
        let Some(rest) = command.key.strip_prefix(prefix) else { continue };
        let mut chars = rest.chars();
        let (Some(key), None) = (chars.next(), chars.next()) else { continue };
        if out.iter().any(|(known, _)| *known == key.to_string()) {
            continue;
        }
        let label = if is_view(command) { command.description.clone() } else { format!("+{}", command.description) };
        out.push((key.to_string(), label));
    }
    out
}

/// The day `org_agenda_start_day` names, read as the date prompt reads it.
fn start_day(text: &str, now: Now, settings: &ReadDateSettings) -> Result<Day, String> {
    let now = DateTime { day: now.today, minute: now.minute };
    let read = read_date::read_date(text, None, now, settings)
        .map_err(|error| format!("org_agenda_start_day `{text}`: {error}"))?;
    Ok(read.date.to_day())
}

/// The agenda settings of a block: `agenda`'s, then the command's
/// `settings`, then the block's.
fn block_config(
    base: &AgendaOptions,
    layers: &[Option<&serde_json::Value>],
    priorities: Config,
) -> Result<(AgendaOptions, Config), String> {
    let mut value = serde_json::to_value(base).map_err(|error| error.to_string())?;
    if let Some(object) = value.as_object_mut() {
        object.remove("org_agenda_custom_commands");
    }
    for layer in layers.iter().flatten() {
        if !layer.is_object() {
            return Err("`settings` must be an object of agenda options".to_string());
        }
        crate::config::merge(&mut value, (*layer).clone());
    }
    let options: AgendaOptions =
        serde_json::from_value(value).map_err(|error| format!("agenda settings of a custom command: {error}"))?;
    let config =
        Config { priorities: priorities.priorities, window_width: priorities.window_width, ..options.to_config() };
    Ok((options, config))
}

fn block_kind(
    kind: &str,
    matching: Option<&str>,
    options: &AgendaOptions,
    config: &Config,
    now: Now,
    read_settings: &ReadDateSettings,
) -> Result<BlockKind, String> {
    let matching = matching.unwrap_or("");
    Ok(match kind {
        "agenda" => {
            let start = options.start_day.as_deref().map(|text| start_day(text, now, read_settings)).transpose()?;
            BlockKind::Agenda { start, span: config.span }
        }
        "alltodo" => BlockKind::Todo { keywords: None },
        "todo" => BlockKind::Todo { keywords: Some(matching.to_string()).filter(|text| !text.trim().is_empty()) },
        "tags" => BlockKind::Tags { matcher: Matcher::new(matching), todo_only: false },
        "tags-todo" => BlockKind::Tags { matcher: Matcher::new(matching), todo_only: true },
        "search" => BlockKind::Search { string: matching.to_string(), todo_only: false },
        other => {
            return Err(format!(
            "Custom agenda commands of type `{other}` are not supported; use agenda, alltodo, todo, tags, tags-todo or search"
        ))
        }
    })
}

/// The blocks of the view `command` shows. `moved` puts one agenda
/// block on other days.
pub fn blocks(
    command: &CustomCommandOption,
    options: &AgendaOptions,
    base: &Config,
    now: Now,
    read_settings: &ReadDateSettings,
    moved: Option<Moved>,
) -> Result<Vec<Block>, String> {
    // A command of one view is a block agenda of one block.
    let blocks = match (&command.blocks, &command.kind) {
        (Some(blocks), _) => blocks.clone(),
        (None, Some(kind)) => vec![CustomBlockOption {
            kind: kind.clone(),
            matching: command.matching.clone(),
            header: command.header.clone(),
            settings: None,
        }],
        (None, None) => return Err(format!("`{}` is a prefix key, not a command", command.key)),
    };
    let mut out = Vec::new();
    for (number, block) in blocks.into_iter().enumerate() {
        let layers = [command.settings.as_ref(), block.settings.as_ref()];
        let (block_options, config) = block_config(options, &layers, base.clone())?;
        let mut kind = block_kind(&block.kind, block.matching.as_deref(), &block_options, &config, now, read_settings)?;
        if let (Some(moved), BlockKind::Agenda { start, span }) =
            (moved.filter(|moved| moved.block == number), &mut kind)
        {
            *start = moved.start;
            *span = moved.span;
        }
        out.push(Block { kind, header: block.header, config });
    }
    Ok(out)
}
