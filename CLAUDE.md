# MarkdownDocs — instructions for agents

MarkdownDocs is an open-source document editor built for AI agents first: Markdown is the native,
canonical working format — read it straight into context, write it straight back out, diff and
patch it like code — not a lossy export bolted onto a GUI app. DOCX, ODT, RTF, HTML, LaTeX and
plain text are first-class import/export targets too, so one tool produces whatever format a task
actually needs. It's a fork of [WordCraft](https://github.com/storytold/wordcraft), a clean-room
Rust word processor, and inherits its document model, layout engine and format converters; where
this file doesn't say otherwise, WordCraft's own engineering discipline still applies here.

## Why this fork exists, and what that changes
WordCraft targets Microsoft Word parity for *people* using a GUI. This project targets the same
underlying documents for *agents*: structural, scriptable, format-flexible. Concretely, that means:
- Markdown gets first-class treatment everywhere a format is checked for fidelity (AutoSave,
  "does this format keep everything" checks, the formats agents are steered toward) — not treated
  as a one-way, lossy export like upstream treats it.
- The command/API surface (CLI, control protocol, MCP — see below) is the primary interface this
  project optimizes for. The GUI (`crates/ui-egui`) is inherited and kept working, but isn't the
  design center the way it is upstream.
- Expect this file, `ROADMAP.md` and the docs under `docs/` to diverge from upstream's over time as
  priorities diverge. When in doubt about which behavior is "correct" for a given feature, prefer
  whatever serves an agent reading/writing the document over what would be more discoverable in a
  ribbon UI.

## Never crash
People and agents both trust this with their documents; a crash loses work. **This outranks feature
work**: never ship a feature through a panic path, and fix a crash before building on top of it.
- **No panics in non-test code:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`,
  `unimplemented!`; no `unsafe` (`unsafe_code = "forbid"`).
- **Errors are `Result<T, E>`** through the crate's error type and `?`. An unfinished feature
  returns an error or is disabled; it never panics.
- **Input-derived numbers are hostile** (files, commands, MCP/control params): `get()` instead of
  `[i]`, slice strings only at char boundaries, checked/saturating arithmetic, clamp sizes, cap
  allocations, bound recursion.
- **Last-resort guard:** `Session::run` turns an escaped panic into an error and restores the
  document. It's a safety net, not a licence.
- **Prove it:** every crash fix lands with a regression test.
  `crates/engine/src/tests.rs::hostile_params_never_panic` runs every command with junk params —
  keep it green.
- Production crate roots carry
  `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`;
  new crates start with it. Tests may `unwrap` (`clippy.toml`).

## Assets: no Microsoft (or Adobe/Avid/Autodesk) iconography or images
**This rule is non-negotiable**, inherited from WordCraft's own clean-room requirement.
- Never copy, trace, screenshot-crop or recreate icons, images, templates, themes, clip art, fonts,
  cursors, sounds or any other asset from Microsoft Word/Office or any Microsoft, Adobe, Avid or
  Autodesk product. Not even "temporarily".
- Every asset must be **original** (drawn in code — prefer this: `crates/ui-egui/src/icons.rs`),
  **public domain / CC0**, **OSI open source**, or **Creative Commons that allows redistribution**,
  or licensed open source by a contributor who made it.
- **Every asset file has a row in [`ATTRIBUTION.md`](ATTRIBUTION.md)** (author, source, licence).
  `cargo xtask assets` (part of `cargo xtask ci`) fails otherwise. Add the row in the same commit as
  the file.
- Documents asking for proprietary fonts (Calibri, Aptos, Cambria…) are rendered with installed
  system fonts or metric-compatible open substitutes (`crates/fonts/src/word.rs`) — never bundle
  proprietary fonts.
- Templates, sample documents and themes are original text and colours
  (`crates/engine/src/sample.rs`, `cmd/design.rs`). Demo images are public domain, with sources
  recorded.

## Clean room
- File formats come from public specs: ECMA-376 / ISO 29500 (OOXML), the published RTF spec, OASIS
  ODF 1.3, CommonMark + GFM for Markdown. Never copy GPL/LGPL/AGPL code (LibreOffice, AbiWord,
  pandoc…).
- Comparisons against real Microsoft Word are fine as black-box observation (run it, screenshot its
  window only, never the whole desktop) when verifying a behavior — but never read, disassemble or
  copy anything inside the Word bundle, and never copy Microsoft wording beyond feature names.

## Architecture
| Layer | Crate | Job |
|---|---|---|
| L0 | `geom` | units, rects, measurement parsing |
| L1 | `doc`, `fonts` | document model + edit primitives; font DB, shaping, Word font substitution |
| L2 | `layout`, `docx`, `formats` | line breaking, pagination, tables, headers/footers, hit testing; OOXML I/O; TXT/MD/HTML/RTF/ODT |
| L3 | `render`, `pdf`, `proof` | vello_cpu rasteriser; krilla PDF; proofing |
| L4 | `engine` | Session, undo, **command registry**, document feature catalog, samples, I/O dispatch |
| L5 | `mcp` | MCP server (headless or bridged to the app) — the primary agent-facing surface |
| L6 | `ui-egui` | the inherited GUI front end (swappable, not the design center for this fork) |
| apps | `apps/wordcraft`, `apps/wordcraft-cli`, `apps/wordcraft-web` | binaries (not yet renamed from upstream) |

- **Everything is a command.** User-visible behaviour = a `CommandSpec` in `crates/engine/src/cmd/*.rs`
  (id, label, ribbon location, shortcut, params doc, `enabled`, `run`) + tests. UI-only commands
  (`ui.*`) live in `crates/ui-egui/src/lib.rs`. The ribbon, shortcuts, command search, CLI, control
  channel and MCP all dispatch by id. Programmatic calls never open dialogs. This is exactly the
  property that makes the engine agent-friendly: every action is already a stable, introspectable,
  JSON-parameterized operation, not something that needs screen-scraping.
- **Layering** is enforced by `cargo xtask layers`. Nothing below L6 depends on egui/eframe/winit/rfd.
  The UI is thin and reads `Session` state; colours come from `theme::Tokens`. This also means a
  fully headless build (CLI + MCP only, no GUI) is architecturally straightforward if that's ever
  worth doing.
- **Rust only** (no handwritten JS/TS). **Never break wasm** (`cargo xtask wasm`).

## Quality gates
Before every commit: `cargo xtask ci` (fmt, clippy -D warnings, tests, assets, layers, wasm). The
pre-existing `fontdb::tests::a_thai_run_with_latin_falls_back_to_one_face` failure is a known,
machine-specific flake inherited from upstream, not a regression — verify with a clean checkout if
unsure. Commit after every arc of work that builds.

## Typing behaves like Word
Typing, Enter, Backspace, Tab, lists, AutoCorrect and AutoFormat follow Word's observed behaviour in
[`docs/typing-parity.md`](docs/typing-parity.md), pinned by `crates/engine/src/tests_typing.rs`
(keys go through the same commands as the keyboard). Change behaviour, doc and tests together.

## Running and looking at the app
- `cargo run --release -p wordcraft -- --sample --control 7981` (sample document + control channel).
- Drive it with JSON lines on `127.0.0.1:7981`, each with the window's key from
  `~/.config/wordcraft/control-key.local-7981` (macOS:
  `~/Library/Application Support/WordCraft/`), e.g.
  `{"id":1,"key":"…","method":"engine.execute","params":{"command":"text.insert","params":{"text":"Hello"}}}`
  then `{"id":2,"key":"…","method":"ui.screenshot","params":{"path":"/tmp/shot.png"}}`. Methods:
  `crates/ui-egui/src/control.rs`; docs: `docs/control-protocol.md`.
- **Headless window screenshots** (no focus stealing, works with a locked screen):
  `cargo run --release -p wordcraft-ui-egui --example ui_shot -- script.jsonl`.
- CLI: `wordcraft-cli convert in.docx out.md`, `wordcraft-cli run --template sample --cmd 'select.text={"text":"Studio"}' --cmd format.bold --save out.docx`,
  `wordcraft-cli commands`.
- MCP: `wordcraft-cli mcp` (headless) or `wordcraft-cli mcp --connect 127.0.0.1:7981` (drives the
  running app). See [`docs/mcp.md`](docs/mcp.md). This is the main way another agent should talk to
  this project.
- Shell gotcha: `mv`/`cp` may be aliased interactive — use `/bin/mv -f` / `/bin/cp -f`.

## Roadmap and progress docs
`ROADMAP.md` (headline numbers, progress log), `docs/target-app-parity.md`, `docs/gaps.md` (ranked
work list), `docs/roadmap.md`, and the parity checklists under `docs/` are all inherited from
WordCraft and describe the state of the inherited feature set honestly. They don't yet reflect this
fork's own Markdown-first priorities — treat gaps there as "inherited from upstream, not yet
reprioritized," not as this project's own roadmap.

## Contributing
- Fork, branch, open a PR. No special scripted contributor-credit process — just commit as
  yourself (or with `Co-Authored-By:` for pairing), including AI agents working on a human's behalf.
- File issues for bugs or gaps, including ones specific to agent use (a command that's awkward to
  drive programmatically, a format round-trip that loses something, MCP/control-protocol rough
  edges) — those are exactly the reports this project most wants.
- See [CONTRIBUTING.md](CONTRIBUTING.md) for the mechanics.
