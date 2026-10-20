//! The documentation website: the Org docs (`README.org`, `docs/`) read
//! with org-lsp's own parser and written as static HTML pages, with a
//! sidebar, an outline of each page and a search.
//!
//! ```text
//! org-site [--out DIR] [--check]
//! org-site --tree FILE
//! ```
//!
//! `--out` is where the site goes (`target/site` by default), emptied
//! first, which it refuses for a directory that is not a site it wrote
//! (one without its `.nojekyll`); `--check`
//! fails when a link points to a file or a heading that is not there.
//! `--tree` prints the syntax tree of a file, to see how it parses.

mod highlight;
mod html;
mod search;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};

use org_syntax::{SyntaxKind, SyntaxNode, TodoKeywords};

/// Where the sources are seen on GitHub, for links to files that are not
/// pages and for "Edit this page".
pub const REPOSITORY: &str = "https://github.com/kristijanhusak/org-lsp";
const BRANCH: &str = "main";

/// The sidebar: its groups, and in each the pages by their source, with
/// the name they have there. Pages not listed come last, under "More".
const NAV: &[(&str, &[(&str, &str)])] = &[
    ("Introduction", &[("README.org", "What org-lsp is")]),
    (
        "Editors",
        &[
            ("docs/neovim.org", "Neovim"),
            ("docs/vim.org", "Vim"),
            ("docs/vscode.org", "VS Code"),
            ("docs/sublime.org", "Sublime Text"),
            ("docs/zed.org", "Zed"),
            ("docs/helix.org", "Helix"),
            ("docs/any-editor.org", "Any other editor"),
        ],
    ),
    (
        "Reference",
        &[
            ("docs/configuration.org", "Configuration"),
            ("docs/plugins.org", "Plugins"),
            ("docs/org-protocol.org", "org-protocol"),
            ("docs/parity.org", "Parity with Emacs"),
        ],
    ),
    (
        "Keys",
        &[
            ("docs/keys/neovim.org", "Neovim"),
            ("docs/keys/vim.org", "Vim"),
            ("docs/keys/vscode.org", "VS Code"),
            ("docs/keys/sublime.org", "Sublime Text"),
            ("docs/keys/zed.org", "Zed"),
            ("docs/keys/helix.org", "Helix"),
        ],
    ),
    (
        "Developers",
        &[
            ("docs/development.org", "Development"),
            ("docs/protocol.org", "The protocol"),
            ("docs/parser.org", "The parser"),
        ],
    ),
];

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match run(&arguments) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("org-site: {error}");
            std::process::exit(1);
        }
    }
}

fn run(arguments: &[String]) -> Result<(), String> {
    let mut out = None;
    let mut check = false;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--out" => out = Some(PathBuf::from(arguments.next().ok_or("--out needs a directory")?)),
            "--check" => check = true,
            "--tree" => {
                let file = arguments.next().ok_or("--tree needs a file")?;
                let text = std::fs::read_to_string(file).map_err(|error| format!("{file}: {error}"))?;
                println!("{:#?}", org_syntax::parse(&text, &TodoKeywords::default()).syntax());
                return Ok(());
            }
            "--help" | "-h" => {
                println!("Usage: org-site [--out DIR] [--check]\n       org-site --tree FILE");
                return Ok(());
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    let root = repository_root()?;
    let out = out.unwrap_or_else(|| root.join("target/site"));
    let site = Site::read(&root)?;
    let problems = site.write(&out)?;
    for problem in &problems {
        eprintln!("{problem}");
    }
    println!("{} pages in {}", site.pages.len(), out.display());
    if check && !problems.is_empty() {
        return Err(format!("{} broken links", problems.len()));
    }
    Ok(())
}

/// The checkout: the directory of the workspace this crate is in.
fn repository_root() -> Result<PathBuf, String> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent().map(Path::to_path_buf).ok_or_else(|| "no repository around the site crate".to_string())
}

/// A page of the site, made from one Org file.
pub struct Page {
    /// The Org file, relative to the checkout (`docs/vim.org`).
    pub source: String,
    /// The HTML file, relative to the site (`vim.html`).
    pub out: String,
    pub title: String,
    /// The group and name of the page in the sidebar.
    pub group: String,
    pub label: String,
    pub root: SyntaxNode,
    /// The headings shown, in order: their level, id and title.
    pub headings: Vec<Heading>,
    pub options: html::Options,
}

impl Page {
    /// The page of the Org file `source`, whose contents are `text`.
    fn new(source: String, group: String, label: String, text: &str) -> Page {
        let root = org_syntax::parse(text, &TodoKeywords::default()).syntax();
        let options = html::Options::read(&root);
        let title = keyword(&root, "title").unwrap_or_else(|| label.clone());
        let headings = headings(&root);
        Page { out: out_path(&source), source, title, group, label, root, headings, options }
    }
}

#[derive(Clone)]
pub struct Heading {
    pub level: usize,
    /// Where its headline starts in the file.
    pub offset: usize,
    pub id: String,
    pub text: String,
}

pub struct Site {
    pub root: PathBuf,
    pub pages: Vec<Page>,
}

impl Site {
    fn read(root: &Path) -> Result<Site, String> {
        let mut sources = vec!["README.org".to_string()];
        for directory in ["docs", "docs/keys"] {
            let mut names: Vec<String> = std::fs::read_dir(root.join(directory))
                .map_err(|error| format!("{directory}: {error}"))?
                .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                .filter(|name| name.ends_with(".org"))
                .map(|name| format!("{directory}/{name}"))
                .collect();
            names.sort();
            sources.extend(names);
        }
        let mut listed: Vec<(String, String, String)> = Vec::new();
        for (group, pages) in NAV {
            for (source, label) in *pages {
                if !sources.iter().any(|known| known == source) {
                    return Err(format!("the sidebar names {source}, which is not there"));
                }
                listed.push((source.to_string(), group.to_string(), label.to_string()));
            }
        }
        for source in &sources {
            if !listed.iter().any(|(known, _, _)| known == source) {
                let name = Path::new(source).file_stem().unwrap().to_string_lossy().into_owned();
                listed.push((source.clone(), "More".to_string(), name));
            }
        }
        let mut pages = Vec::new();
        for (source, group, label) in listed {
            let text = std::fs::read_to_string(root.join(&source)).map_err(|error| format!("{source}: {error}"))?;
            pages.push(Page::new(source, group, label, &text));
        }
        Ok(Site { root: root.to_path_buf(), pages })
    }

    /// Write every page and what they need into `out`; returns the broken
    /// links found on the way.
    fn write(&self, out: &Path) -> Result<Vec<String>, String> {
        clear(out)?;
        std::fs::create_dir_all(out).map_err(|error| format!("{}: {error}", out.display()))?;
        let mut problems = Vec::new();
        let mut files = BTreeSet::new();
        let mut index = Vec::new();
        for (number, page) in self.pages.iter().enumerate() {
            let mut links = Links { site: self, page, problems: &mut problems, files: &mut files };
            let (body, sections) = html::render(page, &mut links);
            index.extend(search::entries(page, sections));
            let document = self.layout(number, &body);
            let path = out.join(&page.out);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(&path, document).map_err(|error| format!("{}: {error}", path.display()))?;
        }
        for file in &files {
            let target = out.join(file);
            std::fs::create_dir_all(target.parent().unwrap()).map_err(|error| error.to_string())?;
            std::fs::copy(self.root.join(file), &target).map_err(|error| format!("{file}: {error}"))?;
        }
        let assets: [(&str, &str); 3] = [
            ("assets/style.css", include_str!("../assets/style.css")),
            ("assets/app.js", include_str!("../assets/app.js")),
            ("assets/logo.svg", include_str!("../assets/logo.svg")),
        ];
        for (name, content) in assets {
            let path = out.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|error| error.to_string())?;
            std::fs::write(&path, content).map_err(|error| error.to_string())?;
        }
        std::fs::write(out.join("assets/search.js"), search::script(&index)).map_err(|error| error.to_string())?;
        std::fs::write(out.join("404.html"), self.not_found()).map_err(|error| error.to_string())?;
        // GitHub Pages serves the files as they are.
        std::fs::write(out.join(MARKER), "").map_err(|error| error.to_string())?;
        Ok(problems)
    }

    /// A page around `body`: the bar at the top, the sidebar, the outline.
    fn layout(&self, number: usize, body: &str) -> String {
        let page = &self.pages[number];
        let up = relative_prefix(&page.out);
        let mut sidebar = String::new();
        let mut groups: BTreeMap<usize, (&str, Vec<&Page>)> = BTreeMap::new();
        let mut order: HashMap<&str, usize> = HashMap::new();
        for other in &self.pages {
            let next = order.len();
            let at = *order.entry(other.group.as_str()).or_insert(next);
            groups.entry(at).or_insert((other.group.as_str(), Vec::new())).1.push(other);
        }
        for (group, pages) in groups.values() {
            sidebar.push_str(&format!("<section><h2>{}</h2><ul>", html::escape(group)));
            for other in pages {
                let current = if other.out == page.out { " class=\"current\" aria-current=\"page\"" } else { "" };
                sidebar.push_str(&format!(
                    "<li><a href=\"{up}{}\"{current}>{}</a></li>",
                    other.out,
                    html::escape(&other.label)
                ));
            }
            sidebar.push_str("</ul></section>");
        }
        let mut outline = String::new();
        for heading in page.headings.iter().filter(|heading| heading.level <= 2) {
            outline.push_str(&format!(
                "<li class=\"level-{}\"><a href=\"#{}\">{}</a></li>",
                heading.level,
                html::escape(&heading.id),
                html::escape(&heading.text)
            ));
        }
        let neighbour = |page: Option<&Page>, rel: &str, label: &str| -> String {
            page.map_or_else(String::new, |page| {
                format!(
                    "<a class=\"{rel}\" href=\"{up}{}\" rel=\"{rel}\"><span>{label}</span>{}</a>",
                    page.out,
                    html::escape(&page.label)
                )
            })
        };
        let previous = neighbour(number.checked_sub(1).and_then(|at| self.pages.get(at)), "prev", "Previous");
        let next = neighbour(self.pages.get(number + 1), "next", "Next");
        let edit = format!("{REPOSITORY}/edit/{BRANCH}/{}", page.source);
        let title = if page.out == "index.html" { page.title.clone() } else { format!("{} | org-lsp", page.title) };
        let outline_block = if outline.is_empty() {
            String::new()
        } else {
            format!("<nav class=\"outline\" aria-label=\"On this page\"><h2>On this page</h2><ul>{outline}</ul></nav>")
        };
        format!(
            r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta name="description" content="org-lsp: Org mode in any editor, through one language server.">
<link rel="icon" href="{up}assets/logo.svg" type="image/svg+xml">
<link rel="stylesheet" href="{up}assets/style.css">
<script>try{{var t=localStorage.getItem('theme');if(t)document.documentElement.dataset.theme=t}}catch(e){{}}</script>
<script defer src="{up}assets/app.js"></script>
</head>
<body data-root="{up}">
<a class="skip" href="#content">Skip to the content</a>
<header class="topbar">
  <button class="menu" type="button" aria-label="Pages" aria-expanded="false" aria-controls="sidebar"><span></span></button>
  <a class="brand" href="{up}index.html"><img src="{up}assets/logo.svg" alt="" width="32" height="32">org-lsp</a>
  <button class="search-open" type="button" aria-label="Search"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg><span>Search</span><kbd>/</kbd></button>
  <nav class="links">
    <a href="{REPOSITORY}" aria-label="GitHub"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8Z"/></svg></a>
    <button class="theme" type="button" aria-label="Theme"><svg class="system" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/></svg><svg class="sun" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg><svg class="moon" viewBox="0 0 24 24" aria-hidden="true"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z"/></svg></button>
  </nav>
</header>
<div class="shell">
<aside class="sidebar" id="sidebar"><nav aria-label="Pages">{sidebar}</nav></aside>
<main id="content">
<article class="doc">
<h1>{page_title}</h1>
{body}
</article>
<footer class="page-footer">
  <a class="edit" href="{edit}">Edit this page on GitHub</a>
  <nav class="neighbours" aria-label="Previous and next pages">{previous}{next}</nav>
  <p class="credit">The unicorn is <a href="https://orgmode.org">Org mode</a>'s logo, by Christophe Bataillon.</p>
</footer>
</main>
{outline_block}
</div>
<div class="search" hidden>
  <div class="search-box" role="dialog" aria-modal="true" aria-label="Search the docs">
    <input type="search" placeholder="Search the docs" aria-label="Search the docs" autocomplete="off" spellcheck="false">
    <ul class="results" role="listbox"></ul>
    <p class="search-help"><kbd>↑</kbd><kbd>↓</kbd> to pick, <kbd>Enter</kbd> to go, <kbd>Esc</kbd> to close</p>
  </div>
</div>
</body>
</html>
"##,
            page_title = html::escape(&page.title),
        )
    }

    fn not_found(&self) -> String {
        // GitHub Pages serves it at any depth: links from the root of the site.
        let root = "/org-lsp/";
        format!(
            r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Not found | org-lsp</title>
<link rel="stylesheet" href="{root}assets/style.css">
</head>
<body class="not-found">
<main>
<h1>Not found</h1>
<p>There is no page here. The <a href="{root}index.html">documentation starts here</a>.</p>
</main>
</body>
</html>
"#
        )
    }
}

/// The value of the first `#+KEY:` line before the first headline.
fn keyword(root: &SyntaxNode, key: &str) -> Option<String> {
    let section = root.children().find(|child| child.kind() == SyntaxKind::SECTION)?;
    section.children().filter(|child| child.kind() == SyntaxKind::KEYWORD).find_map(|node| {
        let mut name = None;
        let mut value = String::new();
        for token in node.children_with_tokens().filter_map(|child| child.into_token()) {
            match token.kind() {
                SyntaxKind::KEYWORD_KEY => {
                    name = Some(token.text().trim_start_matches("#+").trim_end_matches(':').to_lowercase())
                }
                SyntaxKind::KEYWORD_VALUE => value.push_str(token.text()),
                _ => {}
            }
        }
        (name.as_deref() == Some(key)).then(|| value.trim().to_string())
    })
}

/// The headings a page shows, with ids as GitHub gives them (or their
/// `CUSTOM_ID`), which links to them use.
fn headings(root: &SyntaxNode) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for headline in root.descendants().filter(|node| node.kind() == SyntaxKind::HEADLINE) {
        if html::skipped(&headline) {
            continue;
        }
        let headline = org_syntax::ast::Headline::cast(headline).unwrap();
        let title = headline.title();
        // A docs page has no tasks: `* TODO keywords` is a heading about
        // them, as GitHub shows it.
        let keyword = headline.todo_keyword().map(|keyword| format!("{} ", keyword.text())).unwrap_or_default();
        let text = format!("{keyword}{}", title.as_ref().map(html::plain).unwrap_or_default());
        let id = html::custom_id(headline.syntax()).unwrap_or_else(|| {
            let base = slug(&text);
            let count = seen.entry(base.clone()).or_insert(0);
            let id = if *count == 0 { base.clone() } else { format!("{base}-{count}") };
            *count += 1;
            id
        });
        out.push(Heading {
            level: headline.level(),
            offset: usize::from(headline.syntax().text_range().start()),
            id,
            text,
        });
    }
    out
}

/// The anchor GitHub gives a heading: in lower case, without punctuation,
/// spaces as dashes.
pub fn slug(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// Where the page of an Org file goes: `README.org` is the home page, the
/// docs keep their place under `docs/`.
fn out_path(source: &str) -> String {
    if source == "README.org" {
        return "index.html".to_string();
    }
    let path = source.strip_prefix("docs/").unwrap_or(source);
    format!("{}.html", path.strip_suffix(".org").unwrap_or(path))
}

/// `../` as many times as `out` is deep in the site.
fn relative_prefix(out: &str) -> String {
    "../".repeat(out.matches('/').count())
}

/// The links of one page: to other pages and their headings, images
/// copied into the site, the rest of the checkout on GitHub.
pub struct Links<'a> {
    site: &'a Site,
    page: &'a Page,
    problems: &'a mut Vec<String>,
    files: &'a mut BTreeSet<String>,
}

/// What a link becomes.
pub enum Target {
    Href(String),
    Image(String),
}

const IMAGES: &[&str] = &["png", "gif", "jpg", "jpeg", "svg", "webp"];

impl Links<'_> {
    pub fn resolve(&mut self, path: &str) -> Target {
        if let Some(id) = path.strip_prefix('#') {
            self.check_anchor(self.page, id, path);
            return Target::Href(format!("#{id}"));
        }
        if let Some(heading) = path.strip_prefix('*') {
            return Target::Href(format!("#{}", self.heading_id(self.page, heading, path)));
        }
        let file = match path.split_once(':') {
            Some(("file", rest)) => rest,
            Some((scheme, _))
                if scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '+') && !scheme.is_empty() =>
            {
                return Target::Href(path.to_string())
            }
            _ => path,
        };
        let (file, search) = match file.split_once("::") {
            Some((file, search)) => (file, Some(search)),
            None => (file, None),
        };
        let from = Path::new(&self.page.source).parent().unwrap_or(Path::new(""));
        let Some(target) = normalize(&from.join(file)) else {
            self.problems.push(format!("{}: `{path}` points outside the checkout", self.page.source));
            return Target::Href(path.to_string());
        };
        let extension = Path::new(&target).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let up = relative_prefix(&self.page.out);
        if IMAGES.contains(&extension.as_str()) && search.is_none() {
            // One of the site's own files (the logo) is where the site has
            // it already: not copied again.
            if let Some(own) = target.strip_prefix("site/").filter(|_| self.site.root.join(&target).is_file()) {
                return Target::Image(format!("{up}{own}"));
            }
            // Only an image that is there is copied: a missing one is a
            // problem reported with the others, not a failed copy.
            if self.site.root.join(&target).is_file() {
                self.files.insert(target.clone());
            } else {
                self.problems.push(format!("{}: no image `{target}`", self.page.source));
            }
            return Target::Image(format!("{up}{target}"));
        }
        if let Some(other) = self.site.pages.iter().find(|other| other.source == target) {
            let anchor = match search {
                Some(id) if id.starts_with('#') => {
                    self.check_anchor(other, &id[1..], path);
                    id.to_string()
                }
                Some(heading) if heading.starts_with('*') => {
                    format!("#{}", self.heading_id(other, &heading[1..], path))
                }
                _ => String::new(),
            };
            let href = if other.out == self.page.out { String::new() } else { format!("{up}{}", other.out) };
            return Target::Href(format!("{href}{anchor}"));
        }
        let on_disk = self.site.root.join(&target);
        if !on_disk.exists() {
            self.problems.push(format!("{}: no file `{target}`", self.page.source));
        }
        let kind = if on_disk.is_dir() { "tree" } else { "blob" };
        Target::Href(format!("{REPOSITORY}/{kind}/{BRANCH}/{}", target.trim_end_matches('/')))
    }

    fn check_anchor(&mut self, page: &Page, id: &str, path: &str) {
        if !page.headings.iter().any(|heading| heading.id == id) && !html::has_target(&page.root, id) {
            self.problems.push(format!("{}: `{path}`: {} has no heading `#{id}`", self.page.source, page.source));
        }
    }

    fn heading_id(&mut self, page: &Page, title: &str, path: &str) -> String {
        match page.headings.iter().find(|heading| heading.text == title.trim()) {
            Some(heading) => heading.id.clone(),
            None => {
                self.problems.push(format!("{}: `{path}`: {} has no heading `{title}`", self.page.source, page.source));
                slug(title)
            }
        }
    }
}

/// The marker `Site::write` leaves in the site, which says the directory
/// is one it wrote (GitHub Pages reads it too: files served as they are).
const MARKER: &str = ".nojekyll";

/// Empty `out` before the site is written into it: only a site written
/// earlier, so that `--out .` cannot take the checkout with it.
fn clear(out: &Path) -> Result<(), String> {
    let Ok(mut entries) = std::fs::read_dir(out) else {
        return Ok(());
    };
    if entries.next().is_some() && !out.join(MARKER).is_file() {
        return Err(format!(
            "{}: not emptied, as it is not a site written by org-site (it has no {MARKER}); remove it or give another --out",
            out.display()
        ));
    }
    std::fs::remove_dir_all(out).map_err(|error| format!("{}: {error}", out.display()))
}

/// `path` without `.` and `..`, relative to the checkout; `None` above it.
fn normalize(path: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            _ => return None,
        }
    }
    let joined = parts.join("/");
    Some(if path.to_string_lossy().ends_with('/') { format!("{joined}/") } else { joined })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_as_github_makes_them() {
        assert_eq!(slug("The shared configuration file"), "the-shared-configuration-file");
        assert_eq!(slug("Notes (org-roam)"), "notes-org-roam");
        assert_eq!(slug("What works, and why"), "what-works-and-why");
    }

    #[test]
    fn pages_keep_their_place() {
        assert_eq!(out_path("README.org"), "index.html");
        assert_eq!(out_path("docs/vim.org"), "vim.html");
        assert_eq!(out_path("docs/keys/vim.org"), "keys/vim.html");
        assert_eq!(relative_prefix("keys/vim.html"), "../");
        assert_eq!(normalize(Path::new("docs/../editors/vim/TESTING.org")).as_deref(), Some("editors/vim/TESTING.org"));
        assert_eq!(normalize(Path::new("../x")), None);
    }

    /// The body of a page `docs/test.org` holding `text` in this checkout,
    /// the problems found and the files to copy.
    fn render(text: &str) -> (String, Vec<String>, BTreeSet<String>) {
        let page = Page::new("docs/test.org".into(), "More".into(), "test".into(), text);
        let site = Site { root: repository_root().unwrap(), pages: vec![page] };
        let mut problems = Vec::new();
        let mut files = BTreeSet::new();
        let mut links = Links { site: &site, page: &site.pages[0], problems: &mut problems, files: &mut files };
        let (body, _) = html::render(&site.pages[0], &mut links);
        (body, problems, files)
    }

    #[test]
    fn a_broken_link_alone_in_its_paragraph_is_reported_once() {
        let (body, problems, _) = render("[[file:not-there.org]]\n");
        assert_eq!(problems, ["docs/test.org: no file `docs/not-there.org`"]);
        assert!(body.contains("<p><a href="), "{body}");
    }

    #[test]
    fn missing_images_are_reported_not_copied() {
        let (_, problems, files) = render("[[file:images/not-there.png]]\n\nSee [[file:images/not-there.png]].\n");
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(files.is_empty(), "{files:?}");
    }

    #[test]
    fn custom_ids_are_escaped() {
        let (body, problems, _) = render("* A\n:PROPERTIES:\n:CUSTOM_ID: a\"b<c\n:END:\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert!(body.contains("id=\"a&quot;b&lt;c\""), "{body}");
        assert!(body.contains("href=\"#a&quot;b&lt;c\""), "{body}");
    }

    #[test]
    fn only_a_site_it_wrote_is_emptied() {
        let base = std::env::temp_dir().join(format!("org-site-clear-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // Not there, or empty: nothing to keep.
        assert_eq!(clear(&base.join("missing")), Ok(()));
        let empty = base.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(clear(&empty), Ok(()));
        // Anything else is kept, and named in the error.
        let other = base.join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("Cargo.toml"), "").unwrap();
        let error = clear(&other).unwrap_err();
        assert!(error.contains(&other.display().to_string()), "{error}");
        assert!(other.join("Cargo.toml").is_file());
        // A site written earlier goes.
        let site = base.join("site");
        std::fs::create_dir_all(site.join("assets")).unwrap();
        std::fs::write(site.join(MARKER), "").unwrap();
        std::fs::write(site.join("assets/style.css"), "").unwrap();
        assert_eq!(clear(&site), Ok(()));
        assert!(!site.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
