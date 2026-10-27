//! Markdown: a CommonMark subset plus GitHub tables and strikethrough.
//!
//! Import: ATX and setext headings, paragraphs with soft/hard breaks, block quotes (→ Quote
//! style), fenced and indented code (→ one Code paragraph per line), thematic breaks (→ a
//! bottom-border paragraph), bullet and ordered lists (nested by indentation), GFM pipe tables,
//! and inlines: emphasis/strong (the CommonMark delimiter algorithm), `~~strike~~`, code spans,
//! inline links, autolinks, images (`data:` URIs become pictures, others their alt text),
//! backslash escapes, entities, HTML comments (`<!-- ... -->`), `<br>` and `<a id>` anchors.
//!
//! Export writes the same constructs. A picture read from Markdown keeps its destination and is
//! written back as the same `![alt](path)`; any other picture (pasted, from a .docx) is embedded
//! as a `data:` URI, so a Markdown save never points at a file that doesn't exist.
//!
//! Import keeps every image reference: a local file next to the document is loaded when the
//! caller supplies a loader ([`import_on_with`]); otherwise (a URL, a missing file, no loader) the
//! picture is a grey placeholder that still remembers its destination.

use std::collections::HashMap;
use std::sync::Arc;

use wordcraft_doc::{Align, Document};

use crate::html::decode_entity;
use crate::model::{self, Cell, FBlock, FTable, Fmt, Inline, Kind, ListInfo, Para, make_img};

mod blocks;
mod export;
mod inlines;

pub use blocks::{Refs, import, import_on, import_on_with, parse};
pub(crate) use export::escape;
pub use export::export;
#[cfg(test)]
use export::{escape_line_start, inlines_md, table_md, write_blocks};
pub use inlines::parse_inlines;

#[cfg(test)]
mod tests;
