//! The search: each section of each page with its text, written as a
//! script the pages load when the search first opens (a script works
//! from files on disk too, where a page may not fetch another file).

use crate::html::Section;
use crate::Page;

pub struct Entry {
    /// The page's name in the sidebar.
    page: String,
    /// The page, and the heading in it, relative to the site.
    url: String,
    heading: String,
    text: String,
}

/// The longest text kept of a section: enough to find it by.
const TEXT_LIMIT: usize = 4000;

pub fn entries(page: &Page, sections: Vec<Section>) -> Vec<Entry> {
    sections
        .into_iter()
        .map(|section| {
            let mut text = section.text;
            if text.len() > TEXT_LIMIT {
                let mut end = TEXT_LIMIT;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
            }
            Entry {
                page: format!("{} › {}", page.group, page.label),
                url: match &section.id {
                    Some(id) => format!("{}#{id}", page.out),
                    None => page.out.clone(),
                },
                heading: section.heading.unwrap_or_else(|| page.title.clone()),
                text,
            }
        })
        .collect()
}

pub fn script(entries: &[Entry]) -> String {
    let mut out = String::from("window.ORG_SITE_SEARCH = [\n");
    for entry in entries {
        out.push_str(&format!(
            "{{\"page\":{},\"url\":{},\"heading\":{},\"text\":{}}},\n",
            json(&entry.page),
            json(&entry.url),
            json(&entry.heading),
            json(&entry.text)
        ));
    }
    out.push_str("];\n");
    out
}

/// `text` as a JSON string, safe inside a script.
fn json(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '<' => out.push_str("\\u003c"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_strings_stay_inside_the_script() {
        assert_eq!(super::json("a \"b\"\n</script>"), "\"a \\\"b\\\"\\n\\u003c/script>\"");
    }
}
