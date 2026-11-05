//! YAML front matter: the `---` … `---` block at the very start of a Markdown file that static
//! site generators, note apps and agents put metadata in (title, author, tags…).
//!
//! It isn't Markdown, so it isn't read as text: the raw lines are kept in the custom document
//! property [`PROP`], written back verbatim on export, and its plain `title`, `author`,
//! `description`, `subject` and `tags`/`keywords` fill the document's properties. Read as
//! Markdown it was a thematic break, then a Setext heading made of every key.

use crate::model::Meta;

/// The custom document property that holds a Markdown file's front matter, unparsed.
pub const PROP: &str = "MarkdownFrontMatter";

/// Most lines and bytes a front matter block may have; past them the file has none.
const MAX_LINES: usize = 1000;
const MAX_BYTES: usize = 64 * 1024;

/// `text` split into its front matter, if it starts with one (the YAML between the fences, without
/// them), and the Markdown after it. The block opens with a `---` line at the very start and
/// closes with a `---` or `...` line; an opening fence without a close is no front matter.
pub fn split(text: &str) -> (Option<&str>, &str) {
    let body = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let Some(rest) = body.strip_prefix("---\n").or_else(|| body.strip_prefix("---\r\n")) else { return (None, text) };
    let mut at = 0usize;
    for line in rest.split_inclusive('\n').take(MAX_LINES) {
        let bare = line.trim_end_matches(['\n', '\r']);
        if bare == "---" || bare == "..." {
            let yaml = rest.get(..at).unwrap_or("").trim_end_matches(['\n', '\r']);
            let after = rest.get(at + line.len()..).unwrap_or("");
            return (Some(yaml), after);
        }
        at += line.len();
        if at > MAX_BYTES {
            break;
        }
    }
    (None, text)
}

/// The document properties a front matter block's plain keys give: `title`, `author` (or the first
/// of `authors`), `description`, `subject`, and `tags` or `keywords` (a `[a, b]` list or `- a`
/// lines) as comma-separated keywords. Nested and multi-line values are left to the raw text.
pub fn meta(yaml: &str) -> Meta {
    let mut m = Meta::default();
    let lines: Vec<&str> = yaml.lines().collect();
    let mut i = 0;
    while let Some(line) = lines.get(i) {
        i += 1;
        if line.starts_with([' ', '\t', '#']) {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        // A block list (`tags:` then `- a` lines).
        let mut items: Vec<String> = Vec::new();
        if value.is_empty() {
            while let Some(item) = lines.get(i).and_then(|l| l.trim_start().strip_prefix("- ")) {
                items.push(scalar(item));
                i += 1;
            }
        } else if let Some(flow) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            items = flow.split(',').map(scalar).filter(|s| !s.is_empty()).collect();
        }
        let one = if items.is_empty() { scalar(value) } else { items.first().cloned().unwrap_or_default() };
        match key.as_str() {
            "title" => m.title = one,
            "author" | "authors" => m.author = one,
            "description" | "summary" => m.description = one,
            "subject" => m.subject = one,
            "tags" | "keywords" => m.keywords = if items.is_empty() { one } else { items.join(", ") },
            _ => {}
        }
    }
    m
}

/// A plain YAML scalar's text: surrounding quotes taken off, a trailing comment dropped.
fn scalar(v: &str) -> String {
    let v = v.trim();
    for q in ['"', '\''] {
        if let Some(inner) = v.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            return inner.to_string();
        }
    }
    v.split(" #").next().unwrap_or(v).trim().to_string()
}

/// Front matter written back: the fences around `yaml`, then a blank line.
pub fn write(yaml: &str) -> String {
    format!("---\n{yaml}\n---\n\n")
}
