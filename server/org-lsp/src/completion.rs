//! Completion in a headline, as Org's `pcomplete` offers it: TODO
//! keywords for the first word after the stars, and the tags of the file
//! (`org-get-buffer-tags`) after a colon in the headline's tags.

use org_syntax::TodoKeywords;

/// What a completion is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    Tag,
}

/// The candidates at byte `offset` of `text`, and where the word they
/// replace starts.
pub fn complete(text: &str, offset: usize, keywords: &TodoKeywords) -> Option<(usize, Vec<(String, Kind)>)> {
    let offset = offset.min(text.len());
    let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let before = &text[start..offset];
    let stars = before.bytes().take_while(|b| *b == b'*').count();
    if stars == 0 || before.as_bytes().get(stars) != Some(&b' ') {
        return None;
    }
    let rest = &before[stars..];
    // The first word after the stars: a TODO keyword.
    let word = rest.trim_start_matches(' ');
    if !word.contains([' ', '\t', ':']) {
        let words =
            keywords.names().filter(|name| name.starts_with(word)).map(|name| (name.to_string(), Kind::Keyword));
        return Some((offset - word.len(), words.collect()));
    }
    // A tag: the part after the last colon of a tag group that starts
    // after a blank and runs to the cursor.
    let colon = before.rfind(':')?;
    let group_start = before[..=colon].rfind([' ', '\t']).map(|blank| blank + 1)?;
    let group = &before[group_start..offset - start];
    let tag = &before[colon + 1..];
    let valid = |s: &str| s.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%' | ':'));
    if !group.starts_with(':') || !valid(group) {
        return None;
    }
    let present: Vec<&str> = group.split(':').filter(|tag| !tag.is_empty()).collect();
    let mut tags = buffer_tags(text);
    tags.retain(|candidate| candidate.starts_with(tag) && !present.contains(&candidate.as_str()));
    Some((offset - tag.len(), tags.into_iter().map(|tag| (tag, Kind::Tag)).collect()))
}

/// `org-get-buffer-tags`: the tags of every headline, sorted, each once.
fn buffer_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = text
        .lines()
        .filter(|line| line.starts_with('*'))
        .filter_map(|line| {
            let trimmed = line.trim_end();
            let group = trimmed.rsplit([' ', '\t']).next()?;
            (group.len() > 1 && group.starts_with(':') && group.ends_with(':')).then_some(group)
        })
        .flat_map(|group| group.split(':').filter(|tag| !tag.is_empty()).map(str::to_string))
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(marked: &str) -> Option<(usize, Vec<String>)> {
        let offset = marked.find('¦').unwrap();
        let text = marked.replacen('¦', "", 1);
        let keywords = TodoKeywords::from_document(&text).unwrap_or_default();
        complete(&text, offset, &keywords)
            .map(|(start, items)| (start, items.into_iter().map(|(label, _)| label).collect()))
    }

    #[test]
    fn keywords_after_the_stars() {
        assert_eq!(at("* ¦"), Some((2, vec!["TODO".into(), "DONE".into()])));
        assert_eq!(at("** D¦"), Some((3, vec!["DONE".into()])));
        assert_eq!(at("#+TODO: NEXT | DONE\n* N¦"), Some((22, vec!["NEXT".into()])));
        assert_eq!(at("text ¦"), None);
        assert_eq!(at("*bold¦"), None);
    }

    #[test]
    fn tags_of_the_file() {
        let file = "* a :work:home:\n* b :@phone:\n* TODO c :¦\n";
        assert_eq!(at(file), Some((39, vec!["@phone".into(), "home".into(), "work".into()])));
        let file = "* a :work:home:\n* b :wo¦\n";
        assert_eq!(at(file), Some((21, vec!["work".into()])));
        let file = "* a :work:home:\n* b :home:¦\n";
        assert_eq!(at(file), Some((26, vec!["work".into()])));
        assert_eq!(at("* a :work:\n* b text¦"), None);
    }
}
