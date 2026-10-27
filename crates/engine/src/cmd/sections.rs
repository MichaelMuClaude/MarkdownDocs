//! Reading and writing a document by its headings, in Markdown, for agents: `document.sections`
//! lists the headings, `document.markdown` reads the document or one section as Markdown, and
//! `document.setMarkdown` replaces it, or adds to it, from Markdown. A section is a heading and
//! everything under it, up to the next heading of the same or a higher level.

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::{Block, Document, Pos};
use wordcraft_layout::outline::{self as ol, BODY};

use super::sel_result;
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("document.sections", "List Sections", "Agents", |s, _| {
            let levels = ol::levels(&s.doc);
            let sections: Vec<Value> = levels
                .iter()
                .enumerate()
                .filter(|(_, l)| **l != BODY)
                .map(|(i, l)| json!({"block": i, "level": l, "title": title_of(&s.doc, i), "end": ol::subtree_end(&levels, i).max(i + 1)}))
                .collect();
            Ok(json!({"sections": sections, "blocks": s.doc.body.len()}))
        })
        .pure(),
        CommandSpec::new("document.markdown", "Section as Markdown", "Agents", |s, v| {
            let levels = ol::levels(&s.doc);
            let range = find_section(&s.doc, &levels, v)?;
            let (start, end) = range.unwrap_or((0, s.doc.body.len()));
            let mut part = s.doc.clone();
            part.body = s.doc.body.get(start..end).unwrap_or_default().to_vec();
            Ok(json!({"markdown": wordcraft_formats::markdown::export(&part), "block": start, "end": end}))
        })
        .params(r#"{"section"?: a heading's text, or its body block index (see document.sections)}"#)
        .pure(),
        CommandSpec::new("document.setMarkdown", "Set Section from Markdown", "Agents", set_markdown).params(
            r#"{"markdown": string, "section"?: a heading's text, or its body block index (see document.sections), "mode"?: "replace" (default) | "append" (at the end of the section or document) | "before"}"#,
        ),
    ]
}

/// The text of the paragraph at body block `i`.
fn title_of(doc: &Document, i: usize) -> String {
    match doc.body.get(i).map(|b| &**b) {
        Some(Block::Para(p)) => p.final_text().trim().to_string(),
        _ => String::new(),
    }
}

/// The blocks `section` names, `start..end`, or `None` when the command names no section.
fn find_section(doc: &Document, levels: &[u8], v: &Value) -> Result<Option<(usize, usize)>, CmdError> {
    let start = match v.get("section") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(n)) => {
            let i = n.as_u64().and_then(|i| usize::try_from(i).ok()).unwrap_or(usize::MAX);
            if i >= levels.len() {
                return Err(CmdError::Params(format!("`section` {n} is past the last block ({})", levels.len().saturating_sub(1))));
            }
            i
        }
        Some(Value::String(name)) => {
            let want = name.trim().to_lowercase();
            let hits: Vec<usize> =
                (0..levels.len()).filter(|i| levels.get(*i).is_some_and(|l| *l != BODY) && title_of(doc, *i).to_lowercase() == want).collect();
            match hits.as_slice() {
                [one] => *one,
                [] => return Err(CmdError::Params(format!("no heading `{}`: see document.sections", name.trim()))),
                many => {
                    return Err(CmdError::Params(format!("`{}` names {} headings (blocks {many:?}): pass the block index", name.trim(), many.len())));
                }
            }
        }
        Some(_) => return Err(CmdError::Params("`section` is a heading's text or a block index".into())),
    };
    Ok(Some((start, ol::subtree_end(levels, start).max(start + 1))))
}

fn set_markdown(s: &mut Session, v: &Value) -> CmdResult {
    let text = p::req_str(v, "markdown")?;
    let mode = p::str(v, "mode").unwrap_or("replace");
    if !matches!(mode, "replace" | "append" | "before") {
        return Err(CmdError::Params("`mode` is replace, append or before".into()));
    }
    let levels = ol::levels(&s.doc);
    let range = find_section(&s.doc, &levels, v)?;
    let (start, end) = range.unwrap_or((0, s.doc.body.len()));

    let imported = import(s, text);
    let mut blocks: Vec<Block> = imported.body.iter().map(|b| (**b).clone()).collect();
    s.doc.adopt_foreign(&mut blocks, &imported);
    let inserted = blocks.len();

    let (from, to) = match mode {
        "replace" => (start, end),
        "append" => (end, end),
        _ => (start, start),
    };
    s.doc.body.splice(from..to, blocks.into_iter().map(Arc::new));
    // The body ends in a paragraph, never in a table (and is never empty), as Word keeps it.
    if s.doc.body.last().is_none_or(|b| matches!(**b, Block::Table(_))) {
        s.doc.body.push(wordcraft_doc::para_block(wordcraft_doc::Paragraph::new()));
    }
    let at = s.doc.clamp(&Pos::body(from, 0));
    s.sel = Selection::caret(at);
    s.goal_x = None;
    let mut out = sel_result(s)?;
    if let Some(o) = out.as_object_mut() {
        o.insert("block".into(), json!(from));
        o.insert("removed".into(), json!(to - from));
        o.insert("inserted".into(), json!(inserted));
    }
    Ok(out)
}

/// `text` read as Markdown, its pictures loaded from beside the document when it has a file.
fn import(s: &Session, text: &str) -> Document {
    let page = s.doc.last_section.clone();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(dir) = s.path.as_deref().and_then(std::path::Path::parent) {
        let images = crate::io::LocalImages::new(dir);
        return wordcraft_formats::markdown::import_on_with(text, &|src| images.load(src), &page);
    }
    wordcraft_formats::markdown::import_on_with(text, &|_| None, &page)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(md: &str) -> Session {
        Session::new(wordcraft_formats::markdown::import(md))
    }

    fn md(s: &mut Session) -> String {
        s.run("document.markdown", &json!({})).unwrap()["markdown"].as_str().unwrap().to_string()
    }

    const DOC: &str = "# One\n\nfirst.\n\n## Inner\n\ninside.\n\n# Two\n\nsecond.\n";

    /// The headings come back with their level, text and the blocks they cover, so an agent can
    /// address a section without counting paragraphs.
    #[test]
    fn sections_list_the_headings_and_what_they_cover() {
        let mut s = session(DOC);
        let r = s.run("document.sections", &json!({})).unwrap();
        let rows: Vec<(u64, u64, String, u64)> = r["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                (x["block"].as_u64().unwrap(), x["level"].as_u64().unwrap(), x["title"].as_str().unwrap().to_string(), x["end"].as_u64().unwrap())
            })
            .collect();
        assert_eq!(rows, [(0, 1, "One".into(), 4), (2, 2, "Inner".into(), 4), (4, 1, "Two".into(), 6)], "{r}");
    }

    /// A section reads as the Markdown of its heading and what is under it, by name or by block.
    #[test]
    fn a_section_reads_as_markdown() {
        let mut s = session(DOC);
        let by_name = s.run("document.markdown", &json!({"section": "inner"})).unwrap();
        assert_eq!(by_name["markdown"].as_str().unwrap().trim_end(), "## Inner\n\ninside.");
        let by_block = s.run("document.markdown", &json!({"section": 0})).unwrap();
        assert_eq!(by_block["markdown"].as_str().unwrap().trim_end(), "# One\n\nfirst.\n\n## Inner\n\ninside.");
        assert_eq!((by_block["block"].as_u64(), by_block["end"].as_u64()), (Some(0), Some(4)));
        assert_eq!(md(&mut s).trim_end(), DOC.trim_end(), "the whole document");
    }

    /// Replacing a section leaves everything around it alone, and one undo puts it back.
    #[test]
    fn replacing_a_section_changes_only_that_section() {
        let mut s = session(DOC);
        let r =
            s.run("document.setMarkdown", &json!({"section": "Inner", "markdown": "## Inner\n\nrewritten, with **bold**.\n\n- a\n- b\n"})).unwrap();
        assert_eq!((r["removed"].as_u64(), r["inserted"].as_u64(), r["block"].as_u64()), (Some(2), Some(4), Some(2)), "{r}");
        assert_eq!(md(&mut s).trim_end(), "# One\n\nfirst.\n\n## Inner\n\nrewritten, with **bold**.\n\n- a\n- b\n\n# Two\n\nsecond.");
        assert!(s.undo());
        assert_eq!(md(&mut s).trim_end(), DOC.trim_end());
    }

    /// `append` adds at the end of a section (inside it), `before` at its start, and without a
    /// section `replace` is the whole document.
    #[test]
    fn modes_add_around_a_section_or_replace_the_document() {
        let mut s = session(DOC);
        s.run("document.setMarkdown", &json!({"section": "One", "mode": "append", "markdown": "added last."})).unwrap();
        assert_eq!(md(&mut s).trim_end(), "# One\n\nfirst.\n\n## Inner\n\ninside.\n\nadded last.\n\n# Two\n\nsecond.");
        s.run("document.setMarkdown", &json!({"section": "Two", "mode": "before", "markdown": "# Between\n"})).unwrap();
        assert!(md(&mut s).contains("added last.\n\n# Between\n\n# Two"), "{}", md(&mut s));
        s.run("document.setMarkdown", &json!({"markdown": "# Fresh\n\nonly this.\n"})).unwrap();
        assert_eq!(md(&mut s).trim_end(), "# Fresh\n\nonly this.");
    }

    /// Footnotes and pictures in the new text belong to the document they land in: two inserts
    /// that both call their note `[^1]` stay two notes, and the notes keep their text.
    #[test]
    fn notes_and_pictures_come_along_without_colliding() {
        let mut s = session("# A\n\nold.\n");
        s.run("document.setMarkdown", &json!({"section": "A", "mode": "append", "markdown": "one[^1]\n\n[^1]: First note.\n"})).unwrap();
        s.run("document.setMarkdown", &json!({"section": "A", "mode": "append", "markdown": "two[^1]\n\n[^1]: Second note.\n"})).unwrap();
        let out = md(&mut s);
        assert!(out.contains("one[^1]") && out.contains("two[^2]"), "{out}");
        assert!(out.contains("[^1]: First note.") && out.contains("[^2]: Second note."), "{out}");
        let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
        s.run("document.setMarkdown", &json!({"section": "A", "mode": "append", "markdown": format!("![dot]({png})")})).unwrap();
        assert!(md(&mut s).contains("![dot](data:image/png;base64,"), "the picture is in this document's media");
    }

    /// Mistakes are refused with what to do, and change nothing.
    #[test]
    fn mistakes_are_refused_and_change_nothing() {
        let mut s = session("# Same\n\na.\n\n# Same\n\nb.\n");
        let before = md(&mut s);
        for params in [
            json!({"section": "Missing", "markdown": "x"}),
            json!({"section": "Same", "markdown": "x"}),
            json!({"section": 99, "markdown": "x"}),
            json!({"section": true, "markdown": "x"}),
            json!({"section": "Same", "mode": "sideways", "markdown": "x"}),
            json!({"section": 0}),
        ] {
            assert!(s.run("document.setMarkdown", &params).is_err(), "{params}");
        }
        let ambiguous = s.run("document.setMarkdown", &json!({"section": "Same", "markdown": "x"})).unwrap_err().to_string();
        assert!(ambiguous.contains("block index"), "{ambiguous}");
        assert_eq!(md(&mut s), before);
        assert_eq!(s.run("document.setMarkdown", &json!({"section": 2, "markdown": "# Same\n\nc.\n"})).unwrap()["block"], 2);
    }

    /// Lists, code, quotes and tables keep their look when they land in a document that already has
    /// lists of its own: each list is its own, and numbering keeps its start.
    #[test]
    fn lists_code_and_tables_survive_landing_in_another_document() {
        let mut s = session("# A\n\n1. old one\n2. old two\n\n- old bullet\n");
        let new = "3. third\n4. fourth\n\n- new bullet\n\n```rust\nfn main() {}\n```\n\n> quoted\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        s.run("document.setMarkdown", &json!({"section": "A", "mode": "append", "markdown": new})).unwrap();
        let out = md(&mut s);
        for want in
            ["1. old one\n2. old two", "- old bullet", "3. third\n4. fourth", "- new bullet", "```rust\nfn main() {}\n```", "> quoted", "| a | b |"]
        {
            assert!(out.contains(want), "{want:?} missing from:\n{out}");
        }
        // Two separate numbered lists: the new one is not a continuation of the old one.
        let before = s.doc.numbering.nums.len();
        s.run("document.setMarkdown", &json!({"section": "A", "mode": "append", "markdown": "1. again\n"})).unwrap();
        assert_eq!(s.doc.numbering.nums.len(), before + 1);
    }
}
