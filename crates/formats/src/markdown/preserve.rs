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

use super::blocks::{import_chunk, reference_definitions};
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
/// every block that still means what it did is kept as it was. Falls back to [`export`] where
/// footnotes are involved (their numbering is by position) or the file is too large to align.
pub fn export_over(doc: &Document, base: &str) -> String {
    let fresh = export(doc);
    let crlf = base.contains("\r\n");
    let base = base.strip_prefix('\u{FEFF}').unwrap_or(base).replace("\r\n", "\n");
    if base.trim().is_empty() || has_footnote_definition(&base) || has_footnote_definition(&fresh) {
        return fresh;
    }
    let old = split_chunks(&base);
    let new = split_chunks(&fresh);
    let refs: Refs = reference_definitions(&base);
    // Each old block's normal form; empty for one that makes no block (definitions, comments).
    let norm: Vec<String> = old.iter().map(|c| export(&import_chunk(&c.text, &refs)).trim().to_string()).collect();
    let fresh_blocks: Vec<&str> = new.iter().map(|c| c.text.trim()).collect();
    let keep: Vec<usize> = (0..old.len()).filter(|i| norm.get(*i).is_some_and(|n| !n.is_empty())).collect();
    let old_forms: Vec<&str> = keep.iter().filter_map(|i| norm.get(*i)).map(String::as_str).collect();
    let Some(pairs) = align(&old_forms, &fresh_blocks) else { return fresh };
    // New block index → the old block that stands for it.
    let mut stands: Vec<Option<usize>> = vec![None; fresh_blocks.len()];
    for (a, b) in pairs {
        if let (Some(slot), Some(i)) = (stands.get_mut(b), keep.get(a)) {
            *slot = Some(*i);
        }
    }
    let matched_old: std::collections::HashSet<usize> = stands.iter().flatten().copied().collect();
    // Walk both in order: new blocks as they come (an old block's own text where one stands for
    // it), and the old blocks that make no block (reference definitions, comments) where they were.
    let mut pieces: Vec<Piece> = Vec::new();
    let mut next_old = 0usize;
    for (j, block) in fresh_blocks.iter().enumerate() {
        match stands.get(j).copied().flatten() {
            Some(i) => {
                // Pass-through blocks before this old one come first, in their order.
                while next_old < i {
                    take_passthrough(&old, &norm, &matched_old, next_old, &mut pieces);
                    next_old += 1;
                }
                if let Some(c) = old.get(i) {
                    pieces.push(Piece { text: c.text.clone(), old: Some(i) });
                }
                next_old = next_old.max(i + 1);
            }
            None => pieces.push(Piece { text: (*block).to_string(), old: None }),
        }
    }
    while next_old < old.len() {
        take_passthrough(&old, &norm, &matched_old, next_old, &mut pieces);
        next_old += 1;
    }
    let mut out = String::new();
    for (k, piece) in pieces.iter().enumerate() {
        out.push_str(&piece.text);
        out.push('\n');
        if let Some(next) = pieces.get(k + 1) {
            // Two blocks that followed each other in the file keep the blank lines (or none)
            // between them; any other neighbours are one blank line apart.
            let gap = match (piece.old, next.old) {
                (Some(a), Some(b)) if b == a + 1 => old.get(a).map_or(1, |c| c.gap),
                (Some(a), _) => old.get(a).map_or(1, |c| c.gap.max(1)),
                _ => 1,
            };
            out.push_str(&"\n".repeat(gap));
        }
    }
    if crlf { out.replace('\n', "\r\n") } else { out }
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
