# Open upstream PRs ported into MarkdownDocs

Fixes from [storytold/wordcraft](https://github.com/storytold/wordcraft) pull requests that were still **open** (not merged upstream) when they were ported here. Only code and tests were taken, never upstream ROADMAP/docs edits. Each commit keeps the PR author as its author and names the PR in its message.

**When syncing with upstream:** if upstream merges one of these PRs (often revised), drop or revert our commit for it and take upstream's version, so the two don't diverge.

How each was checked:

- **Word-verified**: the PR's claim was reproduced with a synthetic document opened in Microsoft Word (Microsoft 365, version 16.0.20527), observed from outside through COM automation. Our layout was then compared before and after the PR. No Word output is stored in this repository.
- **reviewed+tests**: the diff was read, and the PR's own tests plus our crate tests pass.

## Ported (pilot 2026-11-06)

| PR | Title | Our commit | Check |
|---|---|---|---|
| [#5670](https://github.com/storytold/wordcraft/pull/5670) | Place bottom-aligned floating tables with no vertAnchor at the margin | `39d5847b` | Word-verified: bottom table top 695.6pt vs Word 694.5pt (was 71.5pt) |
| [#5546](https://github.com/storytold/wordcraft/pull/5546) | Different first page: no first-page header on a continuous section's second page | `e1e60a81` | reviewed+tests; import conflict resolved; not Word-checked (needs header parts) |
| [#5346](https://github.com/storytold/wordcraft/pull/5346) | DOCX: a VML text box keeps its text direction (layout-flow) (#5193) | `5dc726b9` | reviewed+tests (VML text box layout-flow); applied cleanly; not Word-checked |
| [#5362](https://github.com/storytold/wordcraft/pull/5362) | Layout: stop a paragraph's border at a floating table beside it | `39233a6c` | reviewed+tests; tests.rs conflict resolved (took theirs); not Word-checked |
| [#5348](https://github.com/storytold/wordcraft/pull/5348) | Layout: a line can break after a hyphen before a slash, like Word (#3408) | `6d5394a4` | Word-verified: line starts match Word in 13/14 paragraphs (was 7/14); the 1 left is a font-width difference; hand-ported into our inline break code |
| [#5347](https://github.com/storytold/wordcraft/pull/5347) | Layout: a picture in a paragraph whose hidden mark closes a section keeps its room (#5288) | `a07b968c` | reviewed+tests; hand-ported into our older pass.rs (quiet_marks); not Word-checked (needs an inline picture) |
| [#5159](https://github.com/storytold/wordcraft/pull/5159) | Layout: an empty list paragraph that closes a continuous section keeps the space it removed, as in Word (#3976) | `d4a3f1b7` | reviewed+tests; flow.rs conflict (comment + y_left) resolved by hand; not Word-checked (needs numbering part) |
| [#5247](https://github.com/storytold/wordcraft/pull/5247) | Layout: wrap Korean Hangul at word spaces and attach brackets (#5238) | `a03aa2d0` | Word-verified: Korean lines start at word spaces like Word (3/4 paragraphs break identically, 0 mid-word splits; before: 5 mid-word splits); kinsoku.rs hand-merged (ours is newer than the PR base) |
| [#4972](https://github.com/storytold/wordcraft/pull/4972) | #4971: chart titles keep the file's manual position, size, weight and colour | `1dfb08e6` | reviewed+tests (chart title manual layout); applied cleanly; not Word-checked |
| [#5071](https://github.com/storytold/wordcraft/pull/5071) | #5065: a DOCX border without w:sz is an eighth of a point | `817dad39` | reviewed+tests; props.rs hand-merged into our newer PropCtx::border (theme colours); not Word-checked |
| [#5064](https://github.com/storytold/wordcraft/pull/5064) | #5056: a DOCX table without a style takes the default table style | `c986b73f` | Word-verified: Word gives a table without w:tblStyle the default table style ('Grid All'); ours now GridAll (was none) |
| [#4879](https://github.com/storytold/wordcraft/pull/4879) | ODT: preserve keyword text across XML events (#4876) | `47856d7b` | reviewed+tests (ODT keywords split by entities); applied cleanly |
| [#4806](https://github.com/storytold/wordcraft/pull/4806) | RTF: preserve text after binary Unicode fallback (#4805) | `443693ec` | reviewed+tests (RTF binary Unicode fallback); applied cleanly |
| [#4789](https://github.com/storytold/wordcraft/pull/4789) | HTML: resolve relative font sizes from the enclosing element | `9577f64b` | reviewed+tests (HTML relative font sizes); applied cleanly |
| [#4874](https://github.com/storytold/wordcraft/pull/4874) | Renumber list items after a deleted item in No Markup | `3a6de379` | reviewed+tests (No Markup renumbers after a deleted item); flow.rs label filter hand-merged |
| [#4656](https://github.com/storytold/wordcraft/pull/4656) | Bound MATH coverage range expansion (#3630) | `53c47317` | reviewed+tests (MATH coverage bounded on malformed fonts); applied cleanly |
| [#4645](https://github.com/storytold/wordcraft/pull/4645) | fix(layout): grow nested equation brackets one size per level | `ccb9e5f1` | reviewed+tests (nested equation brackets grow per level); applied cleanly |
| [#4620](https://github.com/storytold/wordcraft/pull/4620) | Index STYLEREF nearest-style searches (#3730) | `e14e75ae` | reviewed+tests (STYLEREF index, performance); applied cleanly |
| [#4320](https://github.com/storytold/wordcraft/pull/4320) | Fix #4299: DOCX reader cumulative attribute budget across elements | `b4a9f023` | reviewed+tests (DOCX XML attribute budget, hostile input); applied cleanly |
| [#4263](https://github.com/storytold/wordcraft/pull/4263) | Markdown import: an <a> tag checks only the next four bytes for </a> (#3715) | `61d2f955` | reviewed+tests (Markdown <a> check bounded); one-line fix hand-applied to our inlines.rs |
| [#3077](https://github.com/storytold/wordcraft/pull/3077) | Keep page border offsetFrom, display and zOrder on save | `b35cd5ad` | reviewed+tests (page border offsetFrom/display/zOrder kept on save); applied cleanly |
| [#942](https://github.com/storytold/wordcraft/pull/942) | DOCX: keep w:dirty on a field begin marker when saving (#877) | `90a66e6e` | tests only (w:dirty on a field begin marker survives save); applied cleanly |
| [#3587](https://github.com/storytold/wordcraft/pull/3587) | #3584: patterned shading (w:shd pctN, solid) is drawn as the colour it averages to | `c8b757dd` | reviewed+tests (patterned w:shd drawn as its average colour); merged with our nil-shading rule; PR test appended by hand |

## Looked at and skipped

| PR | Title | Why |
|---|---|---|
| [#5545](https://github.com/storytold/wordcraft/pull/5545) | Header Margin anchors: centre in the rendered body, bottom included (#2485 follow-up) | follow-up to upstream #2485 header-margin anchors, not ported to MarkdownDocs (8-file conflict) |
| [#5293](https://github.com/storytold/wordcraft/pull/5293) | Layout: footnotes stay in place below an empty section mark kept on a full page (#3974) | needs upstream's Flow/Fit refactor of flow.rs (ours is pre-refactor); 5-hunk structural conflict |
| [#5252](https://github.com/storytold/wordcraft/pull/5252) | Layout: a continuous section after a page break and an empty closing mark starts the new page (#2109) | same area as 5293, needs upstream flow.rs refactor |
| [#5076](https://github.com/storytold/wordcraft/pull/5076) | #5068: shape text in its run's language, so fonts draw their forms for it (locl) | built on upstream's shape-plan cache (ShapePlan) that our fonts crate lacks |
| [#4779](https://github.com/storytold/wordcraft/pull/4779) | #4778: preserve HTML font emphasis | overlaps 4789 (same author, same apply_style change) |
| [#4836](https://github.com/storytold/wordcraft/pull/4836) | Track Changes: a deleted list item's number is drawn as deleted text (#4828) | built on upstream's LineContext refactor of display/lines.rs (ours draws inline) |
| [#4614](https://github.com/storytold/wordcraft/pull/4614) | Layout: a right tab past the right indent takes its text up to the stop, as in Word | built on upstream's struct refactor of para/lines.rs (ours is pre-refactor) |
| [#4615](https://github.com/storytold/wordcraft/pull/4615) | DOCX: keep ActiveX controls on save and draw a placeholder box (#3597) | our docx writer has no write_shape to hook the ActiveX writer into |
| [#4640](https://github.com/storytold/wordcraft/pull/4640) | Index numbering definition lookups (#3615) | performance-only; conflicts in flow.rs label code and our older tests.rs |
