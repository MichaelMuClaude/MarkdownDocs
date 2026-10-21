# Contributing to MarkdownDocs

Issues and pull requests are welcome from anyone — humans and AI agents working on someone's
behalf alike. This project is explicitly built to be easy for an agent to pick up and work on: the
whole codebase is scriptable (see [AGENTS.md](AGENTS.md)), and contributions that make that *more*
true are especially welcome.

## Filing an issue

Open one at [github.com/MichaelMuClaude/MarkdownDocs/issues](https://github.com/MichaelMuClaude/MarkdownDocs/issues/new/choose).
Useful things to include, roughly in order of how much they help:
- What you did, what you expected, what happened instead.
- The exact command, file or input that triggers it, if there is one — a minimal repro beats a
  description every time.
- For a Markdown or DOCX fidelity gap specifically: the input file (or a minimal snippet) and what
  round-tripping it through this project produces vs. what it should.
- For anything agent-related (MCP, the control protocol, the CLI): which tool/command you called,
  with what params, and what came back.
- Version/commit, OS, and how you built or ran it.

No issue is too small. A one-line "this command's error message is confusing" report is genuinely
useful.

## Contributing code

1. Fork the repo, branch off `main`.
2. Make the change. Keep it scoped — a focused PR reviews faster than a sprawling one.
3. Run `cargo xtask ci` before committing (fmt, clippy -D warnings, tests, the asset-attribution
   check, the layering check, the wasm build). One pre-existing test,
   `fontdb::tests::a_thai_run_with_latin_falls_back_to_one_face`, is a known machine-specific flake
   inherited from upstream — if it's the *only* failure and it also fails on a clean checkout, it's
   not your change.
4. If you fixed a bug, add a regression test, and ideally verify it actually fails without your
   fix (revert the fix locally, confirm the test catches it, then restore the fix) — this is the
   single best way to know a test has real teeth.
5. Open a PR against `main` with a clear description of what changed and why.

See [AGENTS.md](AGENTS.md) for the engineering standards this project holds itself to (the "never
crash" rule in particular is non-negotiable) and the architecture layering rules.

## What's especially welcome right now

- Closing Markdown fidelity gaps against DOCX (task lists, footnotes, reference-style links, and
  whatever else round-trips imperfectly — see `crates/formats/src/markdown.rs`).
- Anything that makes the MCP server or control protocol more complete or easier for an agent to
  drive correctly.
- Bug fixes and test coverage anywhere in the inherited codebase — most of WordCraft's own
  engineering discipline (the command registry, the layering, the never-panic rules) still applies
  here unchanged.

## License

By contributing, you agree your contribution is licensed under this project's MIT OR Apache-2.0
dual license (see [LICENSE-MIT](LICENSE-MIT) / [LICENSE-APACHE](LICENSE-APACHE)), same as the rest
of the codebase.
