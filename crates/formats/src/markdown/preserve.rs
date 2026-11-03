//! Writing a document back over the Markdown it was read from, changing only what changed.
//!
//! A plain [`export`] writes every block in one normal form: lines of a paragraph joined, `*`
//! bullets turned into `-`, `__bold__` into `**bold**`, raw HTML escaped. Read from a file and
//! saved again untouched, that rewrites most of the file, which defeats diffing, patching and
//! reviewing a Markdown file like code. [`export_over`] keeps the original text of every top-level
//! block whose meaning is the same in the document, and writes the normal form only for blocks that
//! are new or changed.
//!
//! A block counts as unchanged when its normal form (the block read and written again on its own)
//! equals a block of the normal form of the document. Blocks are found by splitting at blank lines
//! outside code fences, so a split in the wrong place can only make a block look changed, never
//! write wrong text: the worst case is the plain export.

use std::collections::HashMap;

use super::blocks::{footnote_definitions, import, import_chunk, reference_definitions};
use super::export::export;
use super::{Document, Refs};

/// Most blocks aligned with the full algorithm; past it the plain export is written (the
/// alignment table is blocks × blocks).
const MAX_CELLS: usize = 4_000_000;

/// A block of the output: its text, and the old block it is, when it is one.
struct Piece {
    text: String,
    old: Option<usize>,
}

/// A top-level block of Markdown text: its lines, and the blank lines after it.
struct Chunk {
    text: String,
    gap: usize,
}

/// `doc` as Markdown, written over `base`, the Markdown text it was read from: the text of
/// every block that still means what it did is kept as it was. Falls back to [`export`] where the
/// file is too large to align, or where its footnotes can't be matched up (see [`with_notes`]).
pub fn export_over(doc: &Document, base: &str) -> String {
    let fresh = export(doc);
    let crlf = base.contains("\r\n");
    let base = base.strip_prefix('\u{FEFF}').unwrap_or(base).replace("\r\n", "\n");
    if base.trim().is_empty() {
        return fresh;
    }
    let out = if has_footnote_definition(&base) || has_footnote_definition(&fresh) {
        with_notes(&base, &fresh).unwrap_or_else(|| fresh.clone())
    } else {
        let old = split_chunks(&base);
        let refs: Refs = reference_definitions(&base);
        let norm: Vec<String> = old.iter().map(|c| normal_form(&c.text, &refs)).collect();
        let fresh_blocks: Vec<String> = split_chunks(&fresh).into_iter().map(|c| c.text.trim().to_string()).collect();
        match assemble(&old, &norm, &fresh_blocks, &fresh_blocks) {
            Some(pieces) => write_pieces(&old, &pieces),
            None => fresh.clone(),
        }
    };
    // The blank lines the file ended with stay (a file ending "text\n\n" kept its last one).
    let mut out = out;
    let tail = base.len() - base.trim_end_matches('\n').len();
    while out.len() - out.trim_end_matches('\n').len() < tail {
        out.push('\n');
    }
    if crlf { out.replace('\n', "\r\n") } else { out }
}

/// A chunk's normal form: read and written again on its own; empty for one that makes no block.
fn normal_form(text: &str, refs: &Refs) -> String {
    export(&import_chunk(text, refs)).trim().to_string()
}

/// The pieces of the output: each new block (`fresh_out`, compared by `fresh_cmp`) as it comes,
/// an old block's own text where one stands for it (its normal form `norm` equals the block's),
/// and the old blocks that make no block (definitions, comments) where they were. `None` when the
/// file is too large to align.
fn assemble(old: &[Chunk], norm: &[String], fresh_cmp: &[String], fresh_out: &[String]) -> Option<Vec<Piece>> {
    let keep: Vec<usize> = (0..old.len()).filter(|i| norm.get(*i).is_some_and(|n| !n.is_empty())).collect();
    let old_forms: Vec<&str> = keep.iter().filter_map(|i| norm.get(*i)).map(String::as_str).collect();
    let cmp: Vec<&str> = fresh_cmp.iter().map(String::as_str).collect();
    let pairs = align(&old_forms, &cmp)?;
    // New block index → the old block that stands for it.
    let mut stands: Vec<Option<usize>> = vec![None; cmp.len()];
    for (a, b) in pairs {
        if let (Some(slot), Some(i)) = (stands.get_mut(b), keep.get(a)) {
            *slot = Some(*i);
        }
    }
    let matched_old: std::collections::HashSet<usize> = stands.iter().flatten().copied().collect();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut next_old = 0usize;
    for (j, block) in fresh_out.iter().enumerate() {
        match stands.get(j).copied().flatten() {
            Some(i) => {
                // Pass-through blocks before this old one come first, in their order.
                while next_old < i {
                    take_passthrough(old, norm, &matched_old, next_old, &mut pieces);
                    next_old += 1;
                }
                if let Some(c) = old.get(i) {
                    pieces.push(Piece { text: c.text.clone(), old: Some(i) });
                }
                next_old = next_old.max(i + 1);
            }
            None => pieces.push(Piece { text: block.clone(), old: None }),
        }
    }
    while next_old < old.len() {
        take_passthrough(old, norm, &matched_old, next_old, &mut pieces);
        next_old += 1;
    }
    Some(pieces)
}

/// The pieces as text. Two blocks that followed each other in the file keep the blank lines (or
/// none) between them; any other neighbours are one blank line apart.
fn write_pieces(old: &[Chunk], pieces: &[Piece]) -> String {
    let mut out = String::new();
    for (k, piece) in pieces.iter().enumerate() {
        out.push_str(&piece.text);
        out.push('\n');
        if let Some(next) = pieces.get(k + 1) {
            let gap = match (piece.old, next.old) {
                (Some(a), Some(b)) if b == a + 1 => old.get(a).map_or(1, |c| c.gap),
                (Some(a), _) => old.get(a).map_or(1, |c| c.gap.max(1)),
                _ => 1,
            };
            out.push_str(&"\n".repeat(gap));
        }
    }
    out
}

/// [`export_over`] for a file with footnotes. The plain export numbers notes by position
/// (`[^1]` is the first referred to) and writes their definitions at the end, so blocks are
/// compared with each reference standing for its note's text, not its label:
/// - a kept block keeps its own labels, and the definitions it needs stay where they were;
/// - a new or changed block refers to an existing note by that note's label, and to a new one by
///   a number the file doesn't use yet, whose definition is added at the end;
/// - a definition nothing refers to any more is left out.
///
/// The result is read back and must mean what the plain export does; `None` (write the plain
/// export) when it doesn't, or when a definition stands in the middle of a block.
fn with_notes(base: &str, fresh: &str) -> Option<String> {
    let refs: Refs = reference_definitions(base);
    // The fresh export's notes, by number, and the text before them.
    let (fresh_body, fresh_defs) = footnote_definitions(lines_of(fresh));
    let mut keys: HashMap<String, usize> = HashMap::new();
    let mut key_of_fresh: HashMap<String, String> = HashMap::new();
    let mut body_of_key: HashMap<String, Vec<String>> = HashMap::new();
    for (label, body) in &fresh_defs {
        let key = note_key(body, &refs);
        let n = keys.len();
        keys.entry(key.clone()).or_insert(n);
        key_of_fresh.insert(label.clone(), key.clone());
        body_of_key.entry(key).or_insert_with(|| body.clone());
    }
    let token = |key: &str| keys.get(key).map(|id| format!("QZFNOTE{id}ZQ"));
    // The file's blocks: definitions (their labels) apart from the rest.
    let old = split_chunks(base);
    let mut key_of_old: HashMap<String, String> = HashMap::new();
    let mut def_labels: Vec<Vec<String>> = Vec::with_capacity(old.len());
    for c in &old {
        let (rest, defs) = footnote_definitions(lines_of(&c.text));
        if !defs.is_empty() && rest.iter().any(|l| !l.trim().is_empty()) {
            return None;
        }
        let mut labels = Vec::new();
        for (label, body) in defs {
            key_of_old.insert(label.clone(), note_key(&body, &refs));
            labels.push(label);
        }
        def_labels.push(labels);
    }
    // A reference stands for its note's text; an old note whose text is no note any more gets a
    // token of its own, so the block that refers to it counts as changed.
    let mut old_tokens: HashMap<String, String> = HashMap::new();
    for (label, key) in &key_of_old {
        let t = token(key).unwrap_or_else(|| format!("QZFGONE{}ZQ", old_tokens.len()));
        old_tokens.insert(label.clone(), t);
    }
    let fresh_tokens: HashMap<String, String> = key_of_fresh.iter().filter_map(|(l, k)| Some((l.clone(), token(k)?))).collect();
    let norm: Vec<String> = old
        .iter()
        .zip(&def_labels)
        .map(|(c, labels)| if labels.is_empty() { normal_form(&swap_refs(&c.text, &old_tokens), &refs) } else { String::new() })
        .collect();
    let fresh_chunks: Vec<String> = split_chunks(&fresh_body.join("\n")).into_iter().map(|c| c.text.trim().to_string()).collect();
    let fresh_cmp: Vec<String> = fresh_chunks.iter().map(|b| swap_refs(b, &fresh_tokens)).collect();
    // New and changed blocks refer to the file's label for a note it already has.
    let mut label_of_key: HashMap<String, String> = HashMap::new();
    for (label, key) in &key_of_old {
        label_of_key.entry(key.clone()).or_insert_with(|| label.clone());
    }
    let taken: std::collections::HashSet<String> = key_of_old.keys().cloned().collect();
    let mut next_number = 1usize;
    let mut new_label = |key: &str, label_of_key: &mut HashMap<String, String>| -> String {
        if let Some(l) = label_of_key.get(key) {
            return l.clone();
        }
        while taken.contains(&next_number.to_string()) {
            next_number += 1;
        }
        let l = next_number.to_string();
        next_number += 1;
        label_of_key.insert(key.to_string(), l.clone());
        l
    };
    let mut relabel: HashMap<String, String> = HashMap::new();
    for (fresh_label, key) in &key_of_fresh {
        relabel.insert(fresh_label.clone(), format!("[^{}]", new_label(key, &mut label_of_key)));
    }
    let fresh_out: Vec<String> = fresh_chunks.iter().map(|b| swap_refs(b, &relabel)).collect();
    let mut pieces = assemble(&old, &norm, &fresh_cmp, &fresh_out)?;
    // The labels the text refers to now.
    let all_labels: Vec<String> = key_of_old.keys().chain(label_of_key.values()).cloned().collect();
    let used: std::collections::HashSet<String> = pieces
        .iter()
        .filter(|p| p.old.is_none_or(|i| def_labels.get(i).is_some_and(Vec::is_empty)))
        .flat_map(|p| all_labels.iter().filter(|l| p.text.contains(&format!("[^{l}]"))).cloned().collect::<Vec<_>>())
        .collect();
    // A definition block stays where it was when all its notes are still referred to.
    let mut written: std::collections::HashSet<String> = std::collections::HashSet::new();
    pieces.retain(|p| match p.old.and_then(|i| def_labels.get(i)).filter(|l| !l.is_empty()) {
        Some(labels) if labels.iter().all(|l| used.contains(l)) => {
            written.extend(labels.iter().cloned());
            true
        }
        Some(_) => false,
        None => true,
    });
    let mut out = write_pieces(&old, &pieces);
    // The other notes referred to: definitions at the end, in the plain export's form.
    let mut missing: Vec<(String, &Vec<String>)> = label_of_key
        .iter()
        .filter(|(_, l)| used.contains(*l) && !written.contains(*l))
        .filter_map(|(k, l)| Some((l.clone(), body_of_key.get(k)?)))
        .collect();
    missing.sort_by_key(|(label, _)| natural(label));
    for (label, body) in missing {
        while !out.ends_with("\n\n") {
            out.push('\n');
        }
        let mut it = body.iter();
        out.push_str(format!("[^{label}]: {}", it.next().map_or("", String::as_str)).trim_end());
        out.push('\n');
        for l in it {
            if !l.is_empty() {
                out.push_str("    ");
            }
            out.push_str(l);
            out.push('\n');
        }
    }
    // It must mean what the plain export means.
    (export(&import(&out)) == export(&import(fresh))).then_some(out)
}

/// A note's text in normal form, the same however it was wrapped. (A note with notes of its own
/// refers to them by their labels, which differ between the file and the plain export: its blocks
/// count as changed, and the check at the end decides.)
fn note_key(body: &[String], refs: &Refs) -> String {
    normal_form(&body.join("\n"), refs)
}

/// Sort key for labels: numbers in order, then the rest by text.
fn natural(label: &str) -> (u64, String) {
    (label.parse().unwrap_or(u64::MAX), label.to_string())
}

/// `text` split into lines, as the importer reads them.
fn lines_of(text: &str) -> Vec<String> {
    text.split('\n').map(str::to_string).collect()
}

/// `text` with each footnote reference `[^label]` whose label `map` has replaced by its value.
fn swap_refs(text: &str, map: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(k) = rest.find("[^") {
        let (before, from) = rest.split_at(k);
        out.push_str(before);
        let label = from.get(2..).and_then(|r| r.get(..r.len().min(101))).and_then(|r| r.find(']').and_then(|e| r.get(..e)));
        match label.and_then(|l| map.get(l).map(|v| (l.len(), v))) {
            Some((n, v)) => {
                out.push_str(v);
                rest = from.get(n + 3..).unwrap_or("");
            }
            None => {
                out.push_str("[^");
                rest = from.get(2..).unwrap_or("");
            }
        }
    }
    out.push_str(rest);
    out
}

/// An old block that makes no block of its own (a reference definition, a comment) is kept where
/// it stood; matched and changed blocks are not.
fn take_passthrough(old: &[Chunk], norm: &[String], matched: &std::collections::HashSet<usize>, i: usize, pieces: &mut Vec<Piece>) {
    if norm.get(i).is_some_and(String::is_empty)
        && !matched.contains(&i)
        && let Some(c) = old.get(i)
    {
        pieces.push(Piece { text: c.text.clone(), old: Some(i) });
    }
}

/// Whether `text` has a footnote definition line (`[^1]: …`).
fn has_footnote_definition(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("[^") && t.find("]:").is_some_and(|k| k > 2)
    })
}

/// `text` split into top-level blocks: at blank lines outside code fences, and, with no blank line
/// between (gap 0), where a heading, a code fence, a rule, a block quote or a list starts or ends.
/// A blank line does not end a block when the next line is indented (a loose list item's second
/// paragraph) or when the block is a list and the next line is another item of it.
fn split_chunks(text: &str) -> Vec<Chunk> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut i = 0;
    while let Some(line) = lines.get(i) {
        let t = line.trim_start_matches([' ', '\t']);
        if let Some((c, n)) = fence {
            cur.push(line);
            let run = t.chars().take_while(|x| *x == c).count();
            i += 1;
            if run >= n && t.trim_start_matches(c).trim().is_empty() {
                fence = None;
                // The fence's end ends the block.
                if lines.get(i).is_some_and(|l| !l.trim().is_empty()) {
                    chunks.push(Chunk { text: cur.join("\n"), gap: 0 });
                    cur.clear();
                }
            }
            continue;
        }
        if !line.trim().is_empty() {
            if !cur.is_empty() && starts_new_block(&cur, line) {
                chunks.push(Chunk { text: cur.join("\n"), gap: 0 });
                cur.clear();
            }
            if let Some(f) = open_fence(t) {
                fence = Some(f);
            }
            cur.push(line);
            i += 1;
            continue;
        }
        // Blank lines: how many, and what comes after them.
        let mut j = i;
        while lines.get(j).is_some_and(|l| l.trim().is_empty()) {
            j += 1;
        }
        let next = lines.get(j).copied();
        let continues = match next {
            Some(n) => {
                let indented = n.starts_with("  ") || n.starts_with('\t');
                let list_head = cur.first().is_some_and(|f| is_list_item(f.trim_start()));
                indented || (list_head && is_list_item(n.trim_start()) && !n.starts_with("    "))
            }
            None => false,
        };
        if continues && !cur.is_empty() {
            // Keep the blank lines inside the block.
            for blank in lines.get(i..j).unwrap_or(&[]) {
                cur.push(blank);
            }
        } else if !cur.is_empty() {
            chunks.push(Chunk { text: cur.join("\n"), gap: j - i });
            cur.clear();
        }
        i = j;
    }
    if !cur.is_empty() {
        while cur.last().is_some_and(|l| l.trim().is_empty()) {
            cur.pop();
        }
        chunks.push(Chunk { text: cur.join("\n"), gap: 1 });
    }
    chunks
}

/// Whether `line` (not blank) starts a new block right after the lines `cur` of the block being
/// read, with no blank line between: after a heading or a rule, and at a heading, a code fence, a
/// rule, a block quote after something else, or a list after a paragraph.
fn starts_new_block(cur: &[&str], line: &str) -> bool {
    let last = cur.last().copied().unwrap_or("").trim_start();
    let t = line.trim_start();
    let indented = line.starts_with("  ") || line.starts_with('\t');
    if is_atx_heading(last) || (is_rule(last) && !last.starts_with('-') && !last.starts_with('=')) {
        return true;
    }
    if is_atx_heading(t) || open_fence(t).is_some() || (is_rule(t) && !t.starts_with('-') && !t.starts_with('=')) {
        return true;
    }
    let first = cur.first().copied().unwrap_or("").trim_start();
    if t.starts_with('>') && !last.starts_with('>') {
        return true;
    }
    // A list after a paragraph; an item after a quote line ends the quote's lazy text.
    !indented && is_list_item(t) && !is_list_item(first) && !first.starts_with('>')
}

/// An ATX heading line (`# text`, up to six).
fn is_atx_heading(t: &str) -> bool {
    let n = t.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&n) && t.get(n..).is_some_and(|r| r.is_empty() || r.starts_with(char::is_whitespace))
}

/// A thematic break line: three or more of `-`, `*` or `_` (spaces between allowed), nothing else.
fn is_rule(t: &str) -> bool {
    let mut chars = t.chars().filter(|c| !c.is_whitespace());
    let Some(c) = chars.next().filter(|c| matches!(c, '-' | '*' | '_')) else { return false };
    let rest: Vec<char> = chars.collect();
    rest.len() >= 2 && rest.iter().all(|x| *x == c)
}

/// The fence a line opens (character and length), if it opens one.
fn open_fence(t: &str) -> Option<(char, usize)> {
    let c = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = t.chars().take_while(|x| *x == c).count();
    (n >= 3 && !(c == '`' && t.get(n..).is_some_and(|info| info.contains('`')))).then_some((c, n))
}

/// Whether `t` (without its indent) starts a list item.
fn is_list_item(t: &str) -> bool {
    let mut chars = t.chars();
    match chars.next() {
        Some('-' | '+' | '*') => chars.next().is_some_and(char::is_whitespace),
        Some(d) if d.is_ascii_digit() => {
            let digits = t.chars().take_while(char::is_ascii_digit).count();
            digits <= 9
                && t.get(digits..)
                    .is_some_and(|r| (r.starts_with('.') || r.starts_with(')')) && r.get(1..).is_some_and(|x| x.starts_with(char::is_whitespace)))
        }
        _ => false,
    }
}

/// The pairs `(a, b)` of a longest common subsequence of `a` and `b` (indices into each), or
/// `None` when the alignment table would be too big. The common start and end are matched first.
fn align(a: &[&str], b: &[&str]) -> Option<Vec<(usize, usize)>> {
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let (ra, rb) = (a.get(pre..)?, b.get(pre..)?);
    let suf = ra.iter().rev().zip(rb.iter().rev()).take_while(|(x, y)| x == y).count();
    let (ma, mb) = (ra.get(..ra.len() - suf)?, rb.get(..rb.len() - suf)?);
    if ma.len().saturating_mul(mb.len()) > MAX_CELLS {
        return None;
    }
    let mut pairs: Vec<(usize, usize)> = (0..pre).map(|k| (k, k)).collect();
    let (n, m) = (ma.len(), mb.len());
    let mut t = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let v = if ma.get(i) == mb.get(j) {
                t.get(i + 1).and_then(|r| r.get(j + 1)).copied().unwrap_or(0) + 1
            } else {
                t.get(i + 1).and_then(|r| r.get(j)).copied().unwrap_or(0).max(t.get(i).and_then(|r| r.get(j + 1)).copied().unwrap_or(0))
            };
            if let Some(c) = t.get_mut(i).and_then(|r| r.get_mut(j)) {
                *c = v;
            }
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if ma.get(i) == mb.get(j) {
            pairs.push((pre + i, pre + j));
            i += 1;
            j += 1;
        } else if t.get(i + 1).and_then(|r| r.get(j)).copied().unwrap_or(0) >= t.get(i).and_then(|r| r.get(j + 1)).copied().unwrap_or(0) {
            i += 1;
        } else {
            j += 1;
        }
    }
    let (la, lb) = (a.len(), b.len());
    pairs.extend((0..suf).map(|k| (la - suf + k, lb - suf + k)));
    Some(pairs)
}
