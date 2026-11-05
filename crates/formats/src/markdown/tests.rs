use super::blocks::{code_info, code_mark};
use super::*;

fn inl(s: &str) -> Vec<Inline> {
    parse_inlines(s, &Fmt::default(), 0, &Refs::new())
}

fn spans(s: &str) -> Vec<(String, bool, bool)> {
    inl(s).into_iter().filter_map(|i| if let Inline::Text(t, f) = i { Some((t, f.bold, f.italic)) } else { None }).collect()
}

#[test]
fn emphasis() {
    assert_eq!(spans("a *b* c"), vec![("a ".into(), false, false), ("b".into(), false, true), (" c".into(), false, false)]);
    assert_eq!(spans("**b**"), vec![("b".into(), true, false)]);
    assert_eq!(spans("***x***"), vec![("x".into(), true, true)]);
    assert_eq!(spans("*a **b** c*"), vec![("a ".into(), false, true), ("b".into(), true, true), (" c".into(), false, true)]);
    assert_eq!(spans("snake_case_name"), vec![("snake_case_name".into(), false, false)]);
    assert_eq!(spans("2 * 3 * 4"), vec![("2 * 3 * 4".into(), false, false)]);
    assert_eq!(spans("**unclosed"), vec![("**unclosed".into(), false, false)]);
    let s = inl("~~gone~~");
    assert!(matches!(&s[0], Inline::Text(t, f) if t == "gone" && f.strike));
}

#[test]
fn links_code_escapes() {
    let s = inl("see [the *site*](http://x.org \"T\") and `a*b*` \\*lit\\* &amp; <https://y.com>");
    let link: Vec<_> = s
        .iter()
        .filter_map(|i| if let Inline::Text(t, f) = i { f.link.as_ref().map(|l| (t.clone(), l.to_string(), f.italic)) } else { None })
        .collect();
    assert_eq!(link[0], ("the ".into(), "http://x.org".into(), false));
    assert_eq!(link[1], ("site".into(), "http://x.org".into(), true));
    assert!(link.iter().any(|l| l.1 == "https://y.com"));
    assert!(s.iter().any(|i| matches!(i, Inline::Text(t, f) if t == "a*b*" && f.code)));
    let text: String = s.iter().filter_map(|i| if let Inline::Text(t, _) = i { Some(t.as_str()) } else { None }).collect();
    assert!(text.contains("*lit* & "), "{text}");
}

/// Reference links: `[text][label]`, `[text][]` and `[text]` take their destination from a
/// definition line, and the definition itself leaves the document. Before, none of the three
/// was a link and the definition stayed as a paragraph of its own (#3238).
#[test]
fn reference_links_resolve_and_their_definitions_leave_the_body() {
    let links = |md: &str| -> Vec<(String, String)> {
        match &*parse(md).blocks {
            [FBlock::Para(p), ..] => p
                .inlines
                .iter()
                .filter_map(|x| match x {
                    Inline::Text(t, f) => f.link.as_deref().map(|l| (t.clone(), l.to_string())),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        }
    };
    let def = "\n\n[ref1]: https://example.com/ref\n";
    let to_ref = |text: &str| vec![(text.to_string(), "https://example.com/ref".to_string())];

    assert_eq!(links(&format!("[click here][ref1]{def}")), to_ref("click here"), "full");
    assert_eq!(links(&format!("[ref1][]{def}")), to_ref("ref1"), "collapsed");
    assert_eq!(links(&format!("[ref1]{def}")), to_ref("ref1"), "shortcut");
    // Labels match case-folded with runs of white space collapsed (CommonMark §4.7).
    assert_eq!(links(&format!("[click here][  REF1  ]{def}")), to_ref("click here"), "folded label");
    // An undefined label is not a link: an ordinary bracketed phrase stays text.
    assert_eq!(links(&format!("[nope]{def}")), vec![], "undefined");
    // Inline links are unaffected.
    assert_eq!(links("[x](https://inline.example)"), vec![("x".into(), "https://inline.example".into())], "inline");

    // The definition is not a paragraph, and its text is nowhere in the body.
    let doc = import(&format!("Reference link: [click here][ref1]{def}"));
    let text = doc.plain_text(wordcraft_doc::StoryRef::Body);
    assert_eq!(text, "Reference link: click here", "{text:?}");

    // A definition inside a fence is code, not a definition.
    let fenced = "```\n[ref1]: https://example.com/ref\n```\n\n[ref1]\n";
    let body = import(fenced).plain_text(wordcraft_doc::StoryRef::Body);
    assert!(body.contains("[ref1]: https://example.com/ref"), "the fenced definition is still code: {body:?}");
    assert_eq!(links(fenced), vec![], "a fenced definition defines nothing");
}

/// Cached bracket pairs must not make undefined shortcut references normalize every nested
/// suffix again. One unrelated definition keeps this on the reference-lookup path.
#[test]
fn nested_undefined_reference_labels_have_bounded_work() {
    let literal = format!("prefix {}x{} suffix", "[".repeat(128_000), "]".repeat(128_000));
    let src = format!("{literal}\n\n[other]: https://example.com/other\n");
    let started = std::time::Instant::now();
    let doc = import(&src);
    let elapsed = started.elapsed();
    assert_eq!(doc.plain_text(wordcraft_doc::StoryRef::Body), literal);
    assert!(elapsed < std::time::Duration::from_secs(10), "nested undefined references took {elapsed:?}");
}

#[test]
fn reference_lookup_keeps_long_unicode_labels_and_nested_text() {
    // The work guard is not a label-length cap. Collapsed/full/shortcut references keep their
    // case and whitespace matching, even when lowercasing changes the number of UTF-8 bytes.
    let label = format!("{}İ END", "Écho ".repeat(1500));
    let key = label.to_lowercase();
    let src = format!("[{label}] [{label}][] [full][{label}]\n\n[{key}]: https://example.com/long\n");
    let flow = parse(&src);
    let FBlock::Para(p) = &flow.blocks[0] else { panic!("paragraph") };
    let links: Vec<_> = p
        .inlines
        .iter()
        .filter_map(|x| match x {
            Inline::Text(t, f) if f.link.is_some() => Some((t.as_str(), f.link.as_deref())),
            _ => None,
        })
        .collect();
    assert_eq!(
        links,
        vec![
            (label.as_str(), Some("https://example.com/long")),
            (label.as_str(), Some("https://example.com/long")),
            ("full", Some("https://example.com/long"))
        ]
    );

    let refs = Refs::from([("known".into(), Arc::from("https://example.com/known"))]);
    let out = parse_inlines("[outer [known]] tail [inline](https://example.com/inline)", &Fmt::default(), 0, &refs);
    let text: String = out.iter().filter_map(|x| if let Inline::Text(t, _) = x { Some(t.as_str()) } else { None }).collect();
    assert_eq!(text, "[outer known] tail inline");
    assert!(out.iter().any(|x| matches!(x, Inline::Text(t, f) if t == "known" && f.link.as_deref() == Some("https://example.com/known"))));
    assert!(out.iter().any(|x| matches!(x, Inline::Text(t, f) if t == "inline" && f.link.as_deref() == Some("https://example.com/inline"))));
}

/// A definition has to start a block (CommonMark §4.7), and a fence opens and closes by the
/// same rules the block parser uses. Reading a line without that context deleted text: a
/// definition-shaped line inside a paragraph, and every such line inside a code block whose
/// fence this pass thought had closed.
#[test]
fn a_definition_shaped_line_that_is_not_a_definition_stays_in_the_document() {
    let body = |md: &str| import(md).plain_text(wordcraft_doc::StoryRef::Body);

    // A definition cannot interrupt a paragraph: this is the author's text, lines and all.
    let para = "Status update\n[Warning]: unstable\nmore text\n";
    assert_eq!(body(para), "Status update [Warning]: unstable more text", "{:?}", body(para));

    // `` ```rust `` holds an info string, so it does not close a fence: the definition-shaped
    // line after it is still code.
    let code = "```\n```rust\n[foo]: /bar\n```\n\nSee [foo].\n";
    assert!(body(code).contains("[foo]: /bar"), "{:?}", body(code));
    assert!(body(code).ends_with("See [foo]."), "nothing defines foo: {:?}", body(code));

    // Four spaces of indentation is code, not a fence, so the definition that follows is
    // taken and the reference resolves.
    let indented = "    ```\n\n[x]: /y\n\nSee [x].\n";
    let doc = import(indented);
    let text = doc.plain_text(wordcraft_doc::StoryRef::Body);
    assert!(text.contains("```") && text.ends_with("See x."), "{text:?}");

    // A definition still gives up its line after a blank line, at the start of the document,
    // after a heading, and after another definition.
    let starts = "[a]: /a\n[b]: /b\n\n# Head\n[c]: /c\n\nSee [a], [b], [c].\n";
    assert_eq!(body(starts), "Head\nSee a, b, c.", "{:?}", body(starts));

    // A footnote definition is not a link definition: it becomes the footnote.
    let footnote = import("x[^1]\n\n[^1]: Note\n");
    assert_eq!(footnote.plain_text(wordcraft_doc::StoryRef::Body), "x");
    assert_eq!(footnote.parts.values().filter(|p| p.kind == wordcraft_doc::PartKind::Footnote).count(), 1);
}

#[test]
fn blocks() {
    let f = parse(
        "# Title\n\nPara one\ncontinues.\n\n> quoted\n\n- a\n- b\n    1. nested\n\n```\ncode *x*\n```\n\n---\n\n| A | B |\n|---|:-:|\n| 1 | 2 |\n",
    );
    let kinds: Vec<String> = f
        .blocks
        .iter()
        .map(|b| match b {
            FBlock::Para(p) => format!("{:?}{:?}:{}", p.kind, p.list.map(|l| (l.ordered, l.level)), p.text()),
            FBlock::Table(t) => format!("table{}", t.rows.len()),
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            "Heading(1)None:Title",
            "NormalNone:Para one continues.",
            "QuoteNone:quoted",
            "NormalSome((false, 0)):a",
            "NormalSome((false, 0)):b",
            "NormalSome((true, 1)):nested",
            "CodeNone:code *x*",
            "RuleNone:",
            "table2",
        ]
    );
}

#[test]
fn export_escapes_round_trip() {
    let mut p = Para::default();
    p.push_text("1. not a list *really*", &Fmt::default());
    let md = escape_line_start(inlines_md(&p.inlines));
    let back = parse(&md);
    let FBlock::Para(q) = &back.blocks[0] else { panic!() };
    assert_eq!(q.text(), "1. not a list *really*");
    assert!(q.list.is_none());
}

#[test]
fn export_emphasis_next_to_punctuation_round_trips() {
    let marks: [(&str, fn(&mut Fmt)); 3] = [("bold", |f| f.bold = true), ("italic", |f| f.italic = true), ("strike", |f| f.strike = true)];
    let cases: [&[(&str, bool)]; 4] = [
        &[("注意：", true), ("请阅读", false)],
        &[("a.", true), ("b", false)],
        &[("a", false), ("!", true), ("b", false)],
        &[("a", false), ("(x)", true), ("b", false)],
    ];
    for (name, set) in marks {
        for runs in cases {
            let mut p = Para::default();
            for (t, on) in runs {
                let mut f = Fmt::default();
                if *on {
                    set(&mut f);
                }
                p.push_text(t, &f);
            }
            let md = inlines_md(&p.inlines);
            let back = parse(&md);
            let FBlock::Para(q) = &back.blocks[0] else { panic!() };
            let want: String = runs.iter().map(|r| r.0).collect();
            assert_eq!(q.text(), want, "{name} exported as {md:?}");
            let got: Vec<(String, bool)> = q
                .inlines
                .iter()
                .filter_map(|i| if let Inline::Text(t, f) = i { Some((t.clone(), f.bold || f.italic || f.strike)) } else { None })
                .collect();
            let expect: Vec<(String, bool)> = runs.iter().map(|(t, on)| (t.to_string(), *on)).collect();
            assert_eq!(got, expect, "{name} exported as {md:?}");
        }
    }
}

#[test]
fn export_adjacent_emphasis_next_to_punctuation_round_trips() {
    let b = Fmt { bold: true, ..Fmt::default() };
    let i = Fmt { italic: true, ..Fmt::default() };
    let s = Fmt { strike: true, ..Fmt::default() };
    let cases: [&[(&str, &Fmt)]; 6] = [
        &[("ab", &b), ("!c", &i)],
        &[("ab", &i), ("!c", &b)],
        &[("ab", &b), ("!c", &s)],
        &[("ab", &s), ("(c)", &b)],
        &[("a.", &b), ("b", &i)],
        &[("a.", &b), ("!b", &i)],
    ];
    for runs in cases {
        let mut p = Para::default();
        for (t, f) in runs {
            p.push_text(t, f);
        }
        let md = inlines_md(&p.inlines);
        let back = parse(&md);
        let FBlock::Para(q) = &back.blocks[0] else { panic!() };
        let got: Vec<(String, bool, bool, bool)> =
            q.inlines.iter().filter_map(|x| if let Inline::Text(t, f) = x { Some((t.clone(), f.bold, f.italic, f.strike)) } else { None }).collect();
        let expect: Vec<(String, bool, bool, bool)> = runs.iter().map(|(t, f)| (t.to_string(), f.bold, f.italic, f.strike)).collect();
        assert_eq!(got, expect, "exported as {md:?}");
    }
}

#[test]
fn escape_line_start_preserves_indentation() {
    assert_eq!(escape_line_start("   indented text".to_string()), "   indented text");
    assert_eq!(escape_line_start("   # not a heading".to_string()), "   \\# not a heading");
    assert_eq!(escape_line_start("  1. not a list".to_string()), "  1\\. not a list");
    assert_eq!(escape_line_start("  > quote".to_string()), "  \\> quote");
    // Four spaces or a tab would make an indented code block: the indent is capped at three.
    assert_eq!(escape_line_start("      deep".to_string()), "   deep");
    assert_eq!(escape_line_start("\tTabbed # text".to_string()), "   Tabbed # text");
    assert_eq!(escape_line_start("    # not a heading".to_string()), "   \\# not a heading");
    assert_eq!(escape_line_start(" \t1. not a list".to_string()), "   1\\. not a list");
}

#[test]
fn leading_unicode_whitespace_is_kept_and_does_not_start_blocks() {
    let kinds_text = |md: &str| -> Vec<(Kind, bool, String)> {
        parse(md).blocks.iter().filter_map(|b| if let FBlock::Para(p) = b { Some((p.kind, p.list.is_some(), p.text())) } else { None }).collect()
    };
    assert_eq!(kinds_text("\u{3000}\u{3000}日本語の段落\n"), vec![(Kind::Normal, false, "\u{3000}\u{3000}日本語の段落".to_string())]);
    for src in ["\u{a0}# not a heading", "\u{3000}1. not a list", "\u{a0}- x", "\u{a0}> not a quote"] {
        let mut p = Para::default();
        p.push_text(src, &Fmt::default());
        let md = escape_line_start(inlines_md(&p.inlines));
        assert_eq!(kinds_text(&md), vec![(Kind::Normal, false, src.to_string())], "{src:?} via {md:?}");
    }
}

#[test]
fn single_backtick_code_span_escaped_pipe() {
    let md = "| Command | Status |\n| --- | --- |\n| `cmd \\| grep` | pass |\n";
    let flow = parse(md);
    let FBlock::Table(t) = &flow.blocks[0] else { panic!("expected table") };
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[1].len(), 2);
    let FBlock::Para(p0) = &t.rows[1][0].blocks[0] else { panic!() };
    assert_eq!(p0.inlines.len(), 1);
    assert!(matches!(&p0.inlines[0], Inline::Text(t, f) if t == "cmd | grep" && f.code));
    let FBlock::Para(p1) = &t.rows[1][1].blocks[0] else { panic!() };
    assert_eq!(p1.text(), "pass");

    let mut out = Vec::new();
    table_md(t, &mut out);
    let exported = out.join("\n");
    assert!(exported.contains("`cmd \\| grep`"), "Exported: {exported}");
}

#[test]
fn double_backtick_code_span_escaped_pipe() {
    let md = "| Syntax | Note |\n| --- | --- |\n| ``a \\| b`` | second |\n";
    let flow = parse(md);
    let FBlock::Table(t) = &flow.blocks[0] else { panic!("expected table") };
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[1].len(), 2);
    let FBlock::Para(p0) = &t.rows[1][0].blocks[0] else { panic!() };
    assert_eq!(p0.inlines.len(), 1);
    assert!(matches!(&p0.inlines[0], Inline::Text(t, f) if t == "a | b" && f.code));
    let FBlock::Para(p1) = &t.rows[1][1].blocks[0] else { panic!() };
    assert_eq!(p1.text(), "second");
}

#[test]
fn code_span_literal_backtick_and_escaped_pipe() {
    let md = "| Expression | Detail |\n| --- | --- |\n| ``a`b\\|c`` | distinct |\n";
    let flow = parse(md);
    let FBlock::Table(t) = &flow.blocks[0] else { panic!("expected table") };
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[1].len(), 2);
    let FBlock::Para(p0) = &t.rows[1][0].blocks[0] else { panic!() };
    assert_eq!(p0.inlines.len(), 1);
    assert!(matches!(&p0.inlines[0], Inline::Text(t, f) if t == "a`b|c" && f.code));
    let FBlock::Para(p1) = &t.rows[1][1].blocks[0] else { panic!() };
    assert_eq!(p1.text(), "distinct");
}

#[test]
fn three_column_table_with_distinctive_text() {
    let md = "| Col1 | Col2 | Col3 |\n| --- | --- | --- |\n| `p1\\|q1` | Distinctive Two | Distinctive Three |\n";
    let flow = parse(md);
    let FBlock::Table(t) = &flow.blocks[0] else { panic!("expected table") };
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[1].len(), 3);
    assert_eq!(t.rows[1][0].text(), "p1|q1");
    assert_eq!(t.rows[1][1].text(), "Distinctive Two");
    assert_eq!(t.rows[1][2].text(), "Distinctive Three");
    let FBlock::Para(p0) = &t.rows[1][0].blocks[0] else { panic!() };
    assert!(matches!(&p0.inlines[0], Inline::Text(_, f) if f.code));
    let FBlock::Para(p1) = &t.rows[1][1].blocks[0] else { panic!() };
    assert!(matches!(&p1.inlines[0], Inline::Text(_, f) if !f.code));
    let FBlock::Para(p2) = &t.rows[1][2].blocks[0] else { panic!() };
    assert!(matches!(&p2.inlines[0], Inline::Text(_, f) if !f.code));
}

#[test]
fn regular_table_cells_containing_escaped_pipes() {
    let md = "| Option | Description |\n| --- | --- |\n| Regular \\| Pipe | Normal text cell |\n";
    let flow = parse(md);
    let FBlock::Table(t) = &flow.blocks[0] else { panic!("expected table") };
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[1].len(), 2);
    assert_eq!(t.rows[1][0].text(), "Regular | Pipe");
    assert_eq!(t.rows[1][1].text(), "Normal text cell");
    let FBlock::Para(p0) = &t.rows[1][0].blocks[0] else { panic!() };
    assert!(matches!(&p0.inlines[0], Inline::Text(_, f) if !f.code));

    let mut out = Vec::new();
    table_md(t, &mut out);
    let exported = out.join("\n");
    assert!(exported.contains("Regular \\| Pipe"), "Exported: {exported}");
    assert!(!exported.contains("Regular \\\\| Pipe"), "Exported: {exported}");
}

#[test]
fn unescaped_pipes_behave_as_gfm_column_separators() {
    let md = "| Header 1 | Header 2 |\n| --- | --- |\n| `raw|pipe` | Extra Column |\n";
    let flow = parse(md);
    let FBlock::Table(t) = &flow.blocks[0] else { panic!("expected table") };
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[1].len(), 2);
    assert_eq!(t.rows[1][0].text(), "`raw");
    assert_eq!(t.rows[1][1].text(), "pipe`");
}

#[test]
fn ordinary_tables_and_standalone_code_spans_unchanged() {
    let md = "| Alpha | Beta |\n| --- | --- |\n| 1 | 2 |\n";
    let flow = parse(md);
    let FBlock::Table(t) = &flow.blocks[0] else { panic!() };
    let mut out = Vec::new();
    table_md(t, &mut out);
    assert_eq!(out.join("\n"), "| Alpha | Beta |\n| --- | --- |\n| 1 | 2 |");

    let mut p = Para::default();
    let f = Fmt { code: true, ..Default::default() };
    p.push_text("cmd | grep", &f);
    let exported = inlines_md(&p.inlines);
    assert_eq!(exported, "`cmd | grep`");
}

#[test]
fn code_span_line_endings_stay_literal_on_export() {
    let cases = [
        ("a\n# b", "a # b"),
        ("a\n<div>b</div>", "a <div>b</div>"),
        ("a\n- b", "a - b"),
        ("a\n```", "a ```"),
        ("a\r# b", "a # b"),
        ("a\r\n# b", "a # b"),
        ("a\n\n> b", "a  > b"),
        ("\na\n", " a "),
        ("`a\n# b`", "`a # b`"),
        ("cmd | grep", "cmd | grep"),
    ];
    let shapes = [
        (Kind::Normal, None),
        (Kind::Normal, Some(ListInfo { ordered: false, level: 0, start: 1 })),
        (Kind::Code, Some(ListInfo { ordered: true, level: 0, start: 1 })),
    ];
    for (source, expected) in cases {
        for (kind, list) in shapes {
            let mut p = Para::new(kind);
            p.list = list;
            p.push_text(source, &Fmt { code: true, ..Default::default() });
            let mut doc = model::to_doc(&model::Flow { blocks: vec![FBlock::Para(p)], ..Default::default() });
            // The flow builder removes CR controls; keep the native document's raw input.
            let p = doc.para_mut(wordcraft_doc::StoryRef::Body, &wordcraft_doc::Path::top(0)).unwrap();
            p.text = source.to_string();
            p.runs[0].len = source.len();
            let bytes = crate::export("md", &doc).unwrap().unwrap();
            let md = String::from_utf8(bytes).unwrap();
            assert_eq!(md.trim_end().lines().count(), 1, "{source:?} {kind:?} {list:?} -> {md:?}");
            assert!(!md.contains('\r'), "{md:?}");
            let back = crate::import("md", md.as_bytes()).unwrap().unwrap();
            let flow = model::from_doc(&back);
            let [FBlock::Para(q)] = flow.blocks.as_slice() else { panic!("{md:?}") };
            assert_eq!(q.text(), expected, "{source:?} {kind:?} {list:?} -> {md:?}");
            assert_eq!(q.list, list, "{md:?}");
            assert!(q.inlines.iter().all(|i| matches!(i, Inline::Text(_, f) if f.code)), "{md:?}");
        }
    }
    let fenced = "```\na\n# b\n```";
    assert_eq!(export(&import(fenced)).trim_end(), fenced);
}

#[test]
fn code_span_line_endings_in_table_cells_stay_literal() {
    for ending in ["\n", "\r", "\r\n"] {
        let source = format!("a{ending}# b|c");
        let mut p = Para::default();
        p.push_text(&source, &Fmt { code: true, ..Default::default() });
        let cell = Cell { blocks: vec![FBlock::Para(p)], ..Default::default() };
        let table = FTable { rows: vec![vec![cell, Cell::default()], vec![Cell::default(), Cell::default()]], ..Default::default() };
        let mut doc = model::to_doc(&model::Flow { blocks: vec![FBlock::Table(table)], ..Default::default() });
        let p = doc.para_mut(wordcraft_doc::StoryRef::Body, &wordcraft_doc::Path(vec![0, 0, 0, 0])).unwrap();
        p.text = source.clone();
        p.runs[0].len = source.len();
        let md = export(&doc);
        let back = model::from_doc(&import(&md));
        let [FBlock::Table(table), ..] = back.blocks.as_slice() else { panic!("{md:?}") };
        assert!(back.blocks.iter().skip(1).all(|b| matches!(b, FBlock::Para(p) if p.text().is_empty())), "{md:?}");
        assert_eq!(table.rows.len(), 2, "{md:?}");
        assert_eq!(table.rows[0].len(), 2, "{md:?}");
        let [FBlock::Para(p)] = table.rows[0][0].blocks.as_slice() else { panic!("{md:?}") };
        assert_eq!(p.text(), "a # b|c", "{md:?}");
        assert!(p.inlines.iter().all(|i| matches!(i, Inline::Text(_, f) if f.code)), "{md:?}");
    }
}

#[test]
fn table_code_pipe_export_import_round_trip() {
    let mut table = FTable::default();
    let mut cell_a = Cell::default();
    let mut para_a = Para::default();
    let fmt_a = Fmt { code: true, ..Default::default() };
    para_a.push_text("a`b|c", &fmt_a);
    cell_a.blocks.push(FBlock::Para(para_a));

    let mut cell_b = Cell::default();
    let mut para_b = Para::default();
    para_b.push_text("CRITICAL_DISTINCTIVE_INFO", &Fmt::default());
    cell_b.blocks.push(FBlock::Para(para_b));

    let mut head_a = Cell::default();
    let mut head_para_a = Para::default();
    head_para_a.push_text("Col A", &Fmt::default());
    head_a.blocks.push(FBlock::Para(head_para_a));

    let mut head_b = Cell::default();
    let mut head_para_b = Para::default();
    head_para_b.push_text("Col B", &Fmt::default());
    head_b.blocks.push(FBlock::Para(head_para_b));

    table.rows.push(vec![head_a, head_b]);
    table.rows.push(vec![cell_a, cell_b]);

    let mut out = Vec::new();
    table_md(&table, &mut out);
    let exported_md = out.join("\n");

    assert!(exported_md.contains("``a`b\\|c``"), "Exported: {exported_md}");
    assert!(exported_md.contains("CRITICAL\\_DISTINCTIVE\\_INFO"), "Exported: {exported_md}");

    let back_flow = parse(&exported_md);
    let FBlock::Table(back_table) = &back_flow.blocks[0] else { panic!("expected table") };

    assert_eq!(back_table.rows.len(), 2, "Both rows must exist");
    let row1 = &back_table.rows[1];
    assert_eq!(row1.len(), 2, "Both cells must exist in row 1");

    let back_cell_a = &row1[0];
    let back_cell_b = &row1[1];
    assert_eq!(back_cell_a.text(), "a`b|c", "Cell A text must be preserved");
    assert_eq!(back_cell_b.text(), "CRITICAL_DISTINCTIVE_INFO", "Cell B text must be preserved");

    let FBlock::Para(back_p_a) = &back_cell_a.blocks[0] else { panic!() };
    assert_eq!(back_p_a.inlines.len(), 1);
    assert!(
        matches!(&back_p_a.inlines[0], Inline::Text(t, f) if t == "a`b|c" && f.code),
        "Code formatting must remain intact on Cell A: {:?}",
        back_p_a.inlines
    );

    let FBlock::Para(back_p_b) = &back_cell_b.blocks[0] else { panic!() };
    assert_eq!(back_p_b.inlines.len(), 1);
    assert!(
        matches!(&back_p_b.inlines[0], Inline::Text(t, f) if t == "CRITICAL_DISTINCTIVE_INFO" && !f.code),
        "Cell B formatting must remain plain text: {:?}",
        back_p_b.inlines
    );

    assert_eq!(back_cell_a.text(), "a`b|c");
    assert_eq!(back_cell_b.text(), "CRITICAL_DISTINCTIVE_INFO");

    assert!(!back_cell_a.text().contains("CRITICAL_DISTINCTIVE_INFO"));
    assert!(!back_cell_b.text().contains("a`b|c"));

    let doc = import(&exported_md);
    let re_exported = export(&doc);
    assert!(re_exported.contains("``a`b\\|c``"), "Re-exported: {re_exported}");
    assert!(re_exported.contains("CRITICAL\\_DISTINCTIVE\\_INFO"), "Re-exported: {re_exported}");
    let doc2 = import(&re_exported);
    let back_flow2 = model::from_doc(&doc2);
    let FBlock::Table(back_table2) = &back_flow2.blocks[0] else { panic!() };
    assert_eq!(back_table2.rows[1][0].text(), "a`b|c");
    assert_eq!(back_table2.rows[1][1].text(), "CRITICAL_DISTINCTIVE_INFO");
}

#[test]
fn ordered_list_keeps_start_number() {
    let txt = |md: &str| String::from_utf8_lossy(&crate::txt::export(&import(md))).into_owned();
    assert_eq!(txt("5. a\n6. b"), "5.\ta\r\n6.\tb");
    assert_eq!(txt("3. a\n- b\n4. c"), "3.\ta\r\n•\tb\r\n4.\tc");
    assert_eq!(export(&import("5. a\n6. b")).trim_end(), "5. a\n6. b");
}

/// A quote of several paragraphs stays one quote: the paragraphs were written with a blank line
/// between them, which ends a block quote, so it came back as one quote per paragraph.
#[test]
fn a_quote_of_several_paragraphs_stays_one_quote() {
    let src = "> Hi,\n>\n> First paragraph.\n>\n> Second paragraph.\n\nOutside.\n";
    let md = export(&import(src));
    assert_eq!(md.trim_end(), "> Hi,\n>\n> First paragraph.\n>\n> Second paragraph.\n\nOutside.");
    assert_eq!(export(&import(&md)).trim_end(), md.trim_end(), "and stays so when read back");
    // One quote, not three.
    let quotes = import(&md).body.iter().filter(|b| b.as_para().is_some_and(|p| p.props.style.as_deref() == Some("Quote"))).count();
    assert_eq!(quotes, 3, "three quoted paragraphs");
    assert_eq!(md.matches("\n\n").count(), 1, "one blank line only, before the text outside the quote");
    // A quote followed by a quoted list is one quote too; a list after the quote is not part of it.
    assert_eq!(export(&import("> a\n>\n> - x\n\n- y\n")).trim_end(), "> a\n>\n> - x\n\n- y");
    // Two quotes separated by a paragraph stay two.
    assert_eq!(export(&import("> a\n\nmid\n\n> b\n")).trim_end(), "> a\n\nmid\n\n> b");
}

/// A list inside a block quote stays in the quote: its items were imported as quoted list
/// paragraphs but written back without `>`, so the list moved out of the quote.
#[test]
fn a_list_inside_a_quote_stays_quoted() {
    let src = "> Intro\n>\n> - one\n> - two\n>   - nested\n> 1. first\n\nAfter.\n";
    let want = "> Intro\n>\n> - one\n> - two\n>     - nested\n> 1. first\n\nAfter.";
    let md = export(&import(src));
    assert_eq!(md.trim_end(), want);
    assert_eq!(export(&import(&md)).trim_end(), want, "and stays so when read back");
    // An item's continuation line is quoted too.
    assert_eq!(export(&import("> - a\\\n>   b\n")).trim_end(), "> - a\\\n>   b");
    // A list outside a quote is unchanged.
    assert_eq!(export(&import("- one\n- two\n")).trim_end(), "- one\n- two");
    // A list after a quoted list stays outside the quote.
    let md = export(&import("> - a\n\n2. b\n"));
    assert_eq!(md.trim_end(), "> - a\n\n2. b");
    assert_eq!(export(&import(&md)).trim_end(), "> - a\n\n2. b", "and stays so when read back");
}

#[test]
fn lines_after_line_break_stay_in_paragraph() {
    use wordcraft_doc::{CharProps, Document, Paragraph, StoryRef, para_block};
    let cases = ["Shopping\n- milk\n# not a heading", "a\n1. x", "a\n2) x", "a\n+ x", "a\n---", "a\n***", "a\n==", "a\n--", "a\n> q"];
    for s in cases {
        let mut d = Document::new();
        d.body = vec![para_block(Paragraph::with_text(s, CharProps::default()))];
        let md = export(&d);
        let back = import(&md);
        assert_eq!(back.body.len(), 1, "{s:?} -> {md:?}");
        assert_eq!(back.plain_text(StoryRef::Body), s, "{md:?}");
    }
}

#[test]
fn lines_after_line_break_stay_in_list_item_and_quote() {
    let cases = ["a\n- x", "a\n# h", "a\n1. x", "a\n--", "a\n---", "a\n==", "a\n> q"];
    let shapes: [(Kind, Option<ListInfo>); 5] = [
        (Kind::Normal, Some(ListInfo { ordered: false, level: 0, start: 1 })),
        (Kind::Normal, Some(ListInfo { ordered: true, level: 0, start: 1 })),
        (Kind::Normal, Some(ListInfo { ordered: false, level: 1, start: 1 })),
        (Kind::Normal, Some(ListInfo { ordered: true, level: 1, start: 1 })),
        (Kind::Quote, None),
    ];
    for s in cases {
        for (kind, list) in shapes {
            let mut blocks = Vec::new();
            if let Some(li) = list.filter(|l| l.level > 0) {
                let mut parent = Para::new(Kind::Normal);
                parent.list = Some(ListInfo { ordered: li.ordered, level: 0, start: 1 });
                parent.push_text("top", &Fmt::default());
                blocks.push(FBlock::Para(parent));
            }
            let mut p = Para::new(kind);
            p.list = list;
            p.push_text(s, &Fmt::default());
            blocks.push(FBlock::Para(p));
            let mut out = Vec::new();
            write_blocks(&blocks, &mut out);
            let md = out.join("\n");
            let back = parse(&md);
            assert_eq!(back.blocks.len(), blocks.len(), "{s:?} {kind:?} {list:?} -> {md:?}");
            let Some(FBlock::Para(q)) = back.blocks.last() else { panic!("{md:?}") };
            assert_eq!(q.kind, kind, "{md:?}");
            assert_eq!(q.list, list, "{md:?}");
            assert_eq!(q.text(), s, "{md:?}");
        }
    }
}

#[test]
fn table_column_alignments_import_and_export() {
    let md = "| Left | Center | Right | Default |\n| :--- | :---: | ---: | --- |\n| 1 | 2 | 3 | 4 |\n";
    let flow = parse(md);
    let FBlock::Table(t) = &flow.blocks[0] else { panic!("expected table") };
    assert_eq!(t.rows.len(), 2);
    let aligns: Vec<Option<Align>> =
        t.rows[0].iter().map(|c| c.blocks.iter().find_map(|b| if let FBlock::Para(p) = b { Some(p.align) } else { None }).flatten()).collect();
    assert_eq!(aligns, vec![Some(Align::Left), Some(Align::Center), Some(Align::Right), None]);

    let mut out = Vec::new();
    table_md(t, &mut out);
    let exported = out.join("\n");
    assert!(exported.contains("| :--- | :---: | ---: | --- |"), "Exported: {exported}");
}

#[test]
fn tab_separated_list_markers() {
    let f = parse("-\tbullet item\n*\tstar item\n+\tplus item\n1.\tone item\n2)\ttwo item\n");
    assert_eq!(f.blocks.len(), 5);
    for (i, b) in f.blocks.iter().enumerate() {
        let FBlock::Para(p) = b else { panic!("block {i} not a para") };
        assert!(p.list.is_some(), "block {i} should be a list item: {:?}", p);
    }
    let FBlock::Para(p0) = &f.blocks[0] else { panic!() };
    assert_eq!(p0.text(), "bullet item");
    assert!(!p0.list.unwrap().ordered);

    let FBlock::Para(p3) = &f.blocks[3] else { panic!() };
    assert_eq!(p3.text(), "one item");
    assert!(p3.list.unwrap().ordered);
}

#[test]
fn html_comments_are_ignored_in_blocks_and_inlines() {
    let md = "<!-- block comment -->\n\nHello <!-- inline comment -->world!\n\n<!--\nmultiline\ncomment\n-->\n\nAfter comment.";
    let flow = parse(md);
    assert_eq!(flow.blocks.len(), 2);
    let texts: Vec<String> = flow.blocks.iter().filter_map(|b| if let FBlock::Para(p) = b { Some(p.text()) } else { None }).collect();
    assert_eq!(texts, ["Hello world!", "After comment."]);
}

#[test]
fn a_paragraph_of_delimiters_parses_in_linear_time() {
    // #3714: each delimiter's literal looked back for the text before it, to the start of
    // the paragraph when there was none (quadratic).
    let src = "*_".repeat(200_000);
    let t = std::time::Instant::now();
    let out = inl(&src);
    // Under a second on a loaded machine; the backwards search ran past 6 minutes.
    assert!(t.elapsed() < std::time::Duration::from_secs(10), "{:?}", t.elapsed());
    let text: String = out.iter().filter_map(|i| if let Inline::Text(t, _) = i { Some(t.as_str()) } else { None }).collect();
    assert!(text.chars().all(|c| matches!(c, '*' | '_')), "only delimiters come out");
    // Literal delimiters still take the link of the text before them.
    let s = inl("[a](http://x.org) [b*](http://y.org)");
    assert!(s.iter().any(|i| matches!(i, Inline::Text(t, f) if t == "b*" && f.link.as_deref() == Some("http://y.org"))), "{s:?}");
}

#[test]
fn exported_link_destinations_read_back_unchanged() {
    // Removing `<` and `>` from a destination after the safety check could make a relative
    // target a script or network-path one (`java<script:…` → `javascript:…`). They are
    // escaped instead, as are backslash escapes and entity references a reader would decode.
    let md = |l: &str| inlines_md(&[Inline::Text("x".into(), Fmt { link: Some(l.into()), ..Default::default() })]);
    assert_eq!(md("java<script:alert(1)"), "[x](<java\\<script:alert(1)>)");
    for l in [
        "java<script:alert(1)",
        "/</evil.example/(x)",
        "a>b c",
        "&#106;avascript:alert(1)",
        "&#x6A;avascript:x",
        "&amp;",
        "javascript\\:x",
        "dir\\",
        "dir\\(1)\\",
        "q?a=1&b=2",
    ] {
        let out = md(l);
        let back = inl(&out);
        let got: Vec<_> = back.iter().filter_map(|i| if let Inline::Text(_, f) = i { f.link.as_deref().map(str::to_string) } else { None }).collect();
        assert_eq!(got, [l.to_string()], "{l:?} → {out:?}");
    }
    // Ordinary destinations are written as before.
    assert_eq!(md("https://example.com/a?b=1&c=2"), "[x](https://example.com/a?b=1&c=2)");
    assert_eq!(md("https://example.com/a b"), "[x](<https://example.com/a b>)");
    assert_eq!(md("https://e.com/x\ny"), "[x](https://e.com/xy)");
}

/// A Markdown image is kept on a round trip: its path comes back as written (it used to be
/// read as its alt text alone, and every picture was written as a made-up `image.png`), two
/// paths stay two pictures even with the same placeholder, and a path that needs `<…>` keeps
/// it.
#[test]
fn image_references_survive_a_round_trip() {
    let round = |md: &str| export(&import(md));
    assert_eq!(round("Text ![alt text](pic.png) more.").trim_end(), "Text ![alt text](pic.png) more.");
    assert_eq!(round("![a](<my pic.png>)").trim_end(), "![a](<my pic.png>)");
    assert_eq!(round("![remote](https://example.com/x.png)").trim_end(), "![remote](https://example.com/x.png)");
    let two = round("![one](a.png) ![two](b.png)");
    assert!(two.contains("![one](a.png)") && two.contains("![two](b.png)"), "{two}");
    // An empty destination stays what it was: just the alt text.
    assert_eq!(round("![only alt]()").trim_end(), "only alt");
}

/// A file the loader can read becomes the real picture (shown in the editor), still written
/// back by its path; a picture with no Markdown path (pasted, from a .docx) is embedded as a
/// `data:` URI that reads back as the same picture.
#[test]
fn loaded_images_keep_their_path_and_others_embed() {
    let png = {
        let img = image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    };
    let data = std::sync::Arc::new(png.clone());
    let loader = |src: &str| (src == "pics/dot.png").then(|| data.clone());
    let doc = import_on_with("![dot](pics/dot.png)", &loader, &Default::default());
    assert!(doc.media.values().any(|d| d.as_slice() == png.as_slice()), "the file's own picture is in the document");
    assert_eq!(export(&doc).trim_end(), "![dot](pics/dot.png)");

    let mut doc = import(&format!("![dot](data:image/png;base64,{})", model::base64_encode(&png)));
    // Pasted/DOCX pictures carry an ordinary media key, not a Markdown path.
    let key = doc.media.keys().next().cloned().unwrap();
    let bytes = doc.media.remove(&key).unwrap();
    doc.media.insert("image1.png".into(), bytes);
    for b in &mut doc.body {
        if let wordcraft_doc::Block::Para(p) = std::sync::Arc::make_mut(b) {
            for o in &mut p.objects {
                if let wordcraft_doc::InlineObject::Image { media, .. } = o {
                    *media = "image1.png".into();
                }
            }
        }
    }
    let md = export(&doc);
    assert!(md.starts_with("![dot](data:image/png;base64,"), "{md}");
    let back = import(&md);
    assert!(back.media.values().any(|d| d.as_slice() == png.as_slice()), "the embedded picture reads back unchanged");
}

/// A fenced block's info string (its language) survives, two code blocks in a row stay two,
/// and an empty block isn't dropped. They used to come back as one unlabelled block.
#[test]
fn code_blocks_keep_their_language_and_boundaries() {
    let round = |md: &str| export(&import(md));
    assert_eq!(round("```rust\nfn main() {}\n```\n").trim_end(), "```rust\nfn main() {}\n```");
    let two = round("```python\nprint(1)\n```\n\n    indented\n");
    assert_eq!(two.trim_end(), "```python\nprint(1)\n```\n\n```\nindented\n```", "{two}");
    assert_eq!(round("```c++\nint x;\n```").trim_end(), "```c++\nint x;\n```");
    assert_eq!(round("```{.python}\nx\n```").trim_end(), "```{.python}\nx\n```");
    // Bookmark names stop at Word's 40 characters: a very long info string keeps its start.
    let long = round("```{.python .numberLines startFrom=10}\nx\n```");
    assert!(long.starts_with("```{.python .number") && long.contains("\nx\n```"), "{long}");
    assert_eq!(round("```js\n```").trim_end(), "```js\n\n```", "an empty block keeps a line");
    // Saving the same text twice changes nothing.
    let once = round("```rust\na\n```\n\n```\nb\n```\n");
    assert_eq!(round(&once), once);
    assert_eq!(code_info(&code_mark("objective-c")).as_deref(), Some("objective-c"));
}

/// GFM task lists: `- [ ]` / `- [x]` items are list items with a check-box content control
/// (Word's own, so they toggle in the editor and survive a .docx), and are written back as
/// written. They used to come back as escaped literal text, `- \\[ \\]`.
#[test]
fn task_list_items_round_trip_as_check_boxes() {
    let md = "- [ ] todo\n- [x] done\n- [X] shouted\n- plain\n";
    let doc = import(md);
    let boxes: Vec<bool> = doc
        .body
        .iter()
        .filter_map(|b| b.as_para())
        .filter_map(|p| match p.objects.first() {
            Some(wordcraft_doc::InlineObject::ControlStart { control }) => match control.kind {
                wordcraft_doc::ControlKind::CheckBox { checked, .. } => Some(checked),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(boxes, [false, true, true], "each task item starts with a real check box");
    assert_eq!(export(&doc), "- [ ] todo\n- [x] done\n- [x] shouted\n- plain\n");
    // The box stands in place of the bullet, as GitHub shows task items; a plain item in the
    // same list keeps its bullet, and an ordered task item keeps its number.
    let labels: Vec<bool> = doc.body.iter().filter_map(|b| b.as_para()?.props.numbering).map(|n| doc.numbering.shows_label(n.num, n.level)).collect();
    assert_eq!(labels, [false, false, false, true]);
    let ordered = import("1. [ ] first\n");
    let n = ordered.body.iter().find_map(|b| b.as_para()?.props.numbering).unwrap();
    assert!(ordered.numbering.shows_label(n.num, n.level));
    // Formats without task lists show the box itself, as before.
    let txt = String::from_utf8_lossy(&crate::txt::export(&doc)).into_owned();
    assert_eq!(txt.lines().next().map(str::trim_start), Some("\u{2610} todo"), "{txt}");
    assert_eq!(txt.lines().nth(3), Some("•\tplain"), "{txt}");
    // `[ ]` that isn't followed by a space is ordinary text.
    assert_eq!(export(&import("- [ ]x\n")).trim_end(), "- \\[ \\]x");
}

/// GFM footnotes: a `[^label]` with a definition anywhere in the document is a real footnote
/// (its formatting and paragraphs kept), written back numbered in reference order. An undefined
/// label, or one inside code, stays literal; a definition nothing refers to keeps its text.
#[test]
fn footnotes_round_trip_as_real_notes() {
    let md = "One[^a] two[^b].\n\n[^a]: First *note*.\n[^b]: Second.\n\n    More of it.\n";
    let doc = import(md);
    let notes = doc.parts.values().filter(|p| p.kind == wordcraft_doc::PartKind::Footnote).count();
    assert_eq!(notes, 2, "two real footnotes");
    assert_eq!(export(&doc), "One[^1] two[^2].\n\n[^1]: First *note*.\n\n[^2]: Second.\n\n    More of it.\n");
    assert_eq!(export(&import(&export(&doc))), export(&doc), "stable on a second save");
    assert_eq!(export(&import("No [^such] note.\n")).trim_end(), "No \\[^such\\] note.");
    assert!(export(&import("```\n[^a]: code\n```\n")).contains("[^a]: code"), "inside code it's code");
    let orphan = export(&import("Text.\n\n[^z]: Nobody points here.\n"));
    assert!(orphan.contains("Nobody points here."), "{orphan}");
}

/// The equations of `doc`'s body, in order: (linear text, display).
fn equations(doc: &wordcraft_doc::Document) -> Vec<(String, bool)> {
    doc.body
        .iter()
        .filter_map(|b| b.as_para())
        .flat_map(|p| p.objects.iter())
        .filter_map(|o| match o {
            wordcraft_doc::InlineObject::Equation { linear, display, .. } => Some((linear.clone(), *display)),
            _ => None,
        })
        .collect()
}

/// `$…$` is an inline equation and `$$…$$` a display one, and both are written back as TeX. They
/// used to be read as text and written with the backslashes doubled (`\\frac`).
#[test]
fn dollar_math_round_trips_as_equations() {
    let md = "Energy $E=mc^2$ and $a_1 + b_1$ here.\n\n$$\n\\frac{a}{b}\n$$\n";
    let doc = import(md);
    let eqs = equations(&doc);
    assert_eq!(eqs.iter().map(|(_, d)| *d).collect::<Vec<_>>(), [false, false, true], "{eqs:?}");
    assert_eq!(eqs[2].0, "a/b", "a fraction in the linear format: {eqs:?}");
    let out = export(&doc);
    assert_eq!(out.trim_end(), "Energy $E=mc^2$ and $a_1 + b_1$ here.\n\n$$\na/b\n$$", "{out}");
    // Saving the text again changes nothing.
    assert_eq!(export(&import(&out)), out);
}

/// A display block may hold lines that would open a list, a heading or a quote on their own, and
/// may be written on one line or opened and closed on the lines around it.
#[test]
fn a_display_block_holds_whatever_its_lines_start_with() {
    let doc = import("Before.\n$$\nx\n- y\n# z\n> w\n$$\nAfter.\n");
    let eqs = equations(&doc);
    assert_eq!(eqs.len(), 1, "{eqs:?}");
    assert!(eqs[0].1);
    assert_eq!(doc.plain_text(wordcraft_doc::StoryRef::Body).matches("Before.").count(), 1);
    let one = equations(&import("$$ x^2 $$\n"));
    assert_eq!(one, [("x^2".to_string(), true)]);
    // Never closed: just text, nothing is swallowed.
    assert!(equations(&import("$$\nx\n\nnext paragraph\n")).is_empty());
}

/// Money, escaped dollars and a lone `$` stay text, so a document that talks about prices is not
/// turned into formulas.
#[test]
fn dollars_that_are_not_math_stay_text() {
    for md in ["It costs $5 and $10 today.", "A \\$ sign and another \\$ one.", "Only one $ here.", "$ spaced $ out"] {
        let doc = import(md);
        assert!(equations(&doc).is_empty(), "{md:?}: {:?}", equations(&doc));
        let back = export(&doc);
        assert_eq!(equations(&import(&back)), vec![], "{md:?} -> {back:?}");
    }
    // The closing `$` may not be followed by a digit; a digit inside the delimiters is math.
    assert_eq!(equations(&import("Pay $x$5 now")), vec![]);
    assert_eq!(equations(&import("Cash: $5$")).len(), 1);
    assert_eq!(equations(&import("so $x$.")).len(), 1);
}

/// Inside code, dollars are code; in a table cell an equation is written as text (TeX holds `|`).
#[test]
fn math_is_not_read_in_code_and_not_written_in_cells() {
    assert!(equations(&import("`$x$` and\n\n```\n$y$\n```\n")).is_empty());
    let table = export(&import("$z$\n\n| a |\n|---|\n| b |\n"));
    assert!(table.starts_with("$z$"), "{table}");
}

/// A run of unclosed dollars costs a lookup each, not a rescan: no quadratic blow-up.
#[test]
fn many_unclosed_dollars_are_read_in_linear_time() {
    let md = "$a ".repeat(20_000);
    let t = std::time::Instant::now();
    let doc = import(&md);
    assert!(equations(&doc).is_empty());
    assert!(t.elapsed() < std::time::Duration::from_secs(10), "{:?}", t.elapsed());
}

/// A source with every habit the plain export normalizes away: wrapped paragraphs, `*` bullets,
/// `__bold__`, a raw HTML block, a loose list, a table with its own spacing, a reference link,
/// a Setext heading, two blank lines.
const HABITS: &str = "Setext Heading\n==============\n\nA paragraph wrapped\nover two lines with __bold__ and a [ref link][r].\n\n* star one\n* star two\n\n<div>raw html</div>\n\n- loose one\n\n- loose two\n\n\n| a   | b |\n|:----|--:|\n| 1   | 2 |\n\n> quoted\n> over two lines\n\n```rust\nfn main() {}\n```\n\n[r]: https://example.com \"title\"\n";

/// A document saved over the Markdown it was read from, unchanged, writes that Markdown back
/// byte for byte (the plain export rewrote most of the file).
#[test]
fn saving_an_unchanged_document_over_its_source_changes_nothing() {
    assert_ne!(export(&import(HABITS)), HABITS, "the plain export does normalize this source");
    assert_eq!(export_over(&import(HABITS), HABITS), HABITS);
    // Windows line endings are kept too.
    let crlf = HABITS.replace('\n', "\r\n");
    assert_eq!(export_over(&import(&crlf), &crlf), crlf);
    // No trailing newline in the source: the file ends with one, as the plain export does.
    assert_eq!(export_over(&import("a *b*\nc"), "a *b*\nc"), "a *b*\nc\n");
    // A blank line at the end of the file is kept.
    assert_eq!(export_over(&import("a *b*\n\n"), "a *b*\n\n"), "a *b*\n\n");
}

/// Only the blocks that changed are rewritten; the rest keep their text, however it was written.
#[test]
fn only_changed_blocks_are_rewritten() {
    let mut d = import(HABITS);
    // Change one word of the wrapped paragraph.
    let src = HABITS.replace("wrapped\nover two lines", "wrapped\nover THREE lines");
    let edited = import(&src);
    let out = export_over(&edited, HABITS);
    assert!(out.contains("over THREE lines"), "{out}");
    // The edited paragraph is written in normal form (joined, `**bold**`); nothing else moved.
    assert!(out.contains("A paragraph wrapped over THREE lines with **bold** and a [ref link](https://example.com)."), "{out}");
    for kept in [
        "Setext Heading\n==============",
        "* star one\n* star two",
        "<div>raw html</div>",
        "- loose one\n\n- loose two",
        "| a   | b |\n|:----|--:|",
        "> quoted\n> over two lines",
        "[r]: https://example.com \"title\"",
    ] {
        assert!(out.contains(kept), "{kept:?} kept in {out}");
    }
    // A paragraph appended to the document is added in normal form; one removed is gone.
    d.body.push(wordcraft_doc::para_block(wordcraft_doc::Paragraph::with_text("New last paragraph.", Default::default())));
    let out = export_over(&d, HABITS);
    // The reference definitions at the end of the file stay at the end, after the new paragraph.
    assert!(out.contains("New last paragraph.\n\n[r]: https://example.com"), "{out}");
    assert!(out.contains("* star one\n* star two") && out.contains("<div>raw html</div>"));
    let shorter = "# One\n\nkeep   spaced\n\nlast\n";
    let mut d = import(shorter);
    d.body.remove(1);
    let out = export_over(&d, shorter);
    assert_eq!(out, "# One\n\nlast\n");
}

/// A base that says nothing gives the plain export.
#[test]
fn saving_over_an_empty_source_writes_the_plain_export() {
    let d = import("a\n\nb\n");
    assert_eq!(export_over(&d, ""), export(&d));
    assert_eq!(export_over(&d, "   \n\n"), export(&d));
}

/// A file with footnotes: named labels, a definition in the middle, wrapped lines.
const NOTES: &str = "# Notes\n\nFirst claim[^src] wrapped\nover two lines.\n\n[^src]: The source,\n    on two lines.\n\n* star item\n* another[^2]\n\nLast paragraph.\n\n[^2]: Second note.\n";

/// Saved untouched, a file with footnotes is written back byte for byte (the plain export renumbered
/// the notes, moved every definition to the end and rewrote the rest).
#[test]
fn a_file_with_footnotes_saved_untouched_changes_nothing() {
    assert_ne!(export(&import(NOTES)), NOTES);
    assert_eq!(export_over(&import(NOTES), NOTES), NOTES);
}

/// Editing a file with footnotes rewrites only what changed, and its notes stay right.
#[test]
fn editing_a_file_with_footnotes_rewrites_only_what_changed() {
    // A paragraph without a note changes: the notes, their labels and places are kept.
    let edited = NOTES.replace("Last paragraph.", "Last paragraph, edited.");
    let out = export_over(&import(&edited), NOTES);
    assert_eq!(out, edited);
    // A new paragraph with a new note: the note gets a number the file doesn't use (1, not 2),
    // its definition goes at the end, and the old ones stay where they were (the definitions at
    // the end of the file stay after the text, as reference definitions do).
    let added = format!("{NOTES}\nAdded[^new].\n\n[^new]: Brand new.\n");
    let out = export_over(&import(&added), NOTES);
    let expected = NOTES.replace("Last paragraph.\n", "Last paragraph.\n\nAdded[^1].\n") + "\n[^1]: Brand new.\n";
    assert_eq!(out, expected);
    assert_eq!(export(&import(&out)), export(&import(&added)));
    // A new paragraph that refers to an existing note uses that note's label.
    let again = NOTES.replace("Last paragraph.", "Last paragraph[^2] again.");
    let out = export_over(&import(&again), NOTES);
    assert!(out.contains("Last paragraph[^2] again."), "{out}");
    assert_eq!(export(&import(&out)), export(&import(&again)));
    // The paragraph with a note is removed (in the editor, which takes the note with it): its
    // definition goes too.
    let removed = NOTES.replace("First claim[^src] wrapped\nover two lines.\n\n[^src]: The source,\n    on two lines.\n\n", "");
    let out = export_over(&import(&removed), NOTES);
    assert!(!out.contains("[^src]") && !out.contains("The source"), "{out}");
    assert!(out.contains("* another[^2]") && out.contains("[^2]: Second note."), "{out}");
    assert_eq!(export(&import(&out)), export(&import(&removed)));
    // A note's text changes: the paragraph and the note are written again, still meaning the same.
    let renote = NOTES.replace("Second note.", "Second note, revised.");
    let out = export_over(&import(&renote), NOTES);
    assert!(out.contains("Second note, revised.") && !out.contains("Second note.\n"), "{out}");
    assert!(out.contains("First claim[^src] wrapped\nover two lines."), "{out}");
    assert_eq!(export(&import(&out)), export(&import(&renote)));
}

/// A note that shows a reference as code (`` `[^1]` ``) is an ordinary note; one with a note of its
/// own still saves to what it means.
#[test]
fn footnotes_with_references_in_their_text_save_right() {
    let src = "| a | b |\n|---|---|\n| 1 | 2 |\n\nSee[^1].\n\n[^1]: Written as `[^1]` in Markdown.\n";
    assert_eq!(export_over(&import(src), src), src);
    let nested = "Outer[^a].\n\n[^a]: Has its own[^b].\n\n[^b]: Inner.\n\n| x | y |\n|---|---|\n";
    let out = export_over(&import(nested), nested);
    assert_eq!(export(&import(&out)), export(&import(nested)), "{out}");
}

/// A block only the file holds (a comment, a reference definition) stays where it stood.
#[test]
fn definitions_and_comments_stay_where_they_were() {
    let src = "<!-- keep me -->\n\nFirst [x][a].\n\n[a]: https://a.test\n\nSecond\nline.\n";
    let out = export_over(&import(src), src);
    assert_eq!(out, src);
}

/// Paragraphs and code blocks inside list items stay inside them (they were joined to the item's
/// first paragraph with line breaks, losing the code block), numbering goes on after them, and a
/// paragraph after a nested list belongs to the outer item (it became a new item).
#[test]
fn blocks_inside_list_items_keep_their_place() {
    for src in [
        "- Step one:\n\n  ```bash\n  ls -la\n\n  echo done\n  ```\n\n- Step two\n\n  Second paragraph of two.\n",
        "1. First\n\n   More about the first.\n\n2. Second\n",
        "- Outer\n    - Inner\n\n      Inside inner.\n\n  Back in outer.\n\n- Next\n",
        "- ```\n  code first\n  ```\n",
    ] {
        assert_eq!(export(&import(src)), src);
    }
    let d = import("1. First\n\n   More.\n\n2. Second\n");
    // The second paragraph is no list item of its own: two items, numbered 1 and 2.
    let numbered = d.body.iter().filter(|b| matches!(&***b, wordcraft_doc::Block::Para(p) if p.props.numbering.is_some())).count();
    assert_eq!(numbered, 2);
}

/// A code block inside a block quote stays in the quote (it came out after it, unquoted).
#[test]
fn a_code_block_in_a_quote_stays_quoted() {
    let src = "> Before:\n>\n> ```rust\n> fn main() {}\n> ```\n>\n> After.\n";
    assert_eq!(export(&import(src)), src);
    // Saved over itself, nothing changes either.
    assert_eq!(export_over(&import(src), src), src);
}

/// A heading that is bold all through is written without `**` (a web page's headings carry bold
/// of their own: `# **Title**` came out of every saved page); part-bold headings keep theirs.
#[test]
fn an_all_bold_heading_is_written_plain() {
    assert_eq!(export(&import("# **Saved page**\n")), "# Saved page\n");
    assert_eq!(export(&import("## Part **bold**\n")), "## Part **bold**\n");
}

/// Hostile tables: a 63-column header over 200,000 one-cell rows made 12.6 million padded cells;
/// the document now stops at its cell budget. A header wider than Word's 63 columns keeps 63.
#[test]
fn huge_markdown_tables_are_bounded() {
    let header = format!("|{}\n|{}\n", "h|".repeat(63), "-|".repeat(63));
    let src = format!("{header}{}", "|x|\n".repeat(200_000));
    let started = std::time::Instant::now();
    let d = import(&src);
    let cells: usize = d
        .body
        .iter()
        .filter_map(|b| if let wordcraft_doc::Block::Table(t) = &**b { Some(t.rows.iter().map(|r| r.cells.len()).sum::<usize>()) } else { None })
        .sum();
    assert!(cells > 0 && cells <= 1 << 18, "{cells} cells");
    assert!(started.elapsed().as_secs() < 60, "took {:?}", started.elapsed());
    let wide = format!("|{}\n|{}\n|{}\n", "h|".repeat(100), "-|".repeat(100), "c|".repeat(100));
    let d = import(&wide);
    let cols = d.body.iter().find_map(|b| if let wordcraft_doc::Block::Table(t) = &**b { t.rows.first().map(|r| r.cells.len()) } else { None });
    assert_eq!(cols, Some(63));
}
