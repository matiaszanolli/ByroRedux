# PEX-D4-2026-09-22-01: seven more newline-skipping peek()/check() sites glue adjacent lines with zero errors — #4472 fixed six, left at least seven

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Labels**: medium, scripting, bug
- **Source**: docs/audits/AUDIT_PAPYRUS_2026-09-22.md (PEX-D4-2026-09-22-01)
- **GitHub issue**: https://github.com/matiaszanolli/ByroRedux/issues/4763

## Description

`352455afd` (#4472) fixed six newline-skipping `peek()`/`check()` decision sites in
`crates/papyrus/src/parser/{mod,stmt,script}.rs`, switching them to `Parser::check_raw`/
`peek_raw` so a malformed continuation line no longer silently glues into the previous
single-line construct. A systematic sweep of every remaining `self.peek()`/
`self.check(&Token::…)` call site in `crates/papyrus/src/parser/{mod,script,stmt,expr}.rs`
found seven more sites of the identical defect, none touched by #4472's fix. Re-verified
against HEAD `c3f298a24` at publish time — every line number matched the file exactly, no
drift:

- `crates/papyrus/src/parser/script.rs:101` — `Extends` header clause (`parse_script_header`)
- `crates/papyrus/src/parser/script.rs:259` — `parse_variable_flags`
- `crates/papyrus/src/parser/script.rs:366` — `parse_function_flags`
- `crates/papyrus/src/parser/script.rs:396` — `parse_property` initializer
- `crates/papyrus/src/parser/script.rs:701` — `parse_group` flags
- `crates/papyrus/src/parser/stmt.rs:211` — `parse_var_decl_or_expr` initializer
- `crates/papyrus/src/parser/stmt.rs:245` — `parse_variable_body` initializer (backs both
  `stmt.rs:185` local keyword-typed decls and `script.rs:676` struct members)

## Evidence

Verified by direct execution against a throwaway probe crate (path-dependency on
`byroredux-papyrus`, deleted after use) — full per-site probe inputs and results in the
GitHub issue body and in `docs/audits/AUDIT_PAPYRUS_2026-09-22.md`. Re-confirmed by direct
code read at HEAD `c3f298a24`: every cited site still calls the newline-skipping `self.peek()`
(contrast with the six #4472-fixed sites, which carry `// #4472 — raw: …` comments and call
`peek_raw()`/`check_raw()`).

## Impact

Malformed `.psc` source is silently accepted as a different, plausible-but-wrong AST instead
of a recovered parse error. Reachable via `crates/scripting/examples/extender_preflight.rs`'s
`scan_source` (offline compatibility scan over on-disk `.psc` files) and the localhost debug
console's expression evaluator. No runtime/game-load path affected.

## Related

#4472 (fixed six sibling sites, incomplete as a class fix), #4321 (original class), #2656
(comparable sibling that was fixed correctly).

## Suggested Fix

Apply `check_raw`/`peek_raw` to all seven sites, one regression test per site following
`crates/papyrus/src/parser/script.rs:836-946`. `parse_variable_body` backs two call sites so
fixing it clears both in one edit; `parse_var_decl_or_expr` needs its own fix. Consider a
standing grep-based lint against reintroducing this class.

## Validation notes (audit-publish, 2026-09-22)

- Classified CONFIRMED against current code — not STALE, not a duplicate of any open or closed
  issue (checked against both the shared `/tmp/audit/issues.json` dedup cache, 4,650 issues
  through #4762, and a freshly pulled open-issue list, 246 open issues).
- All 7 cited line numbers read back exactly as reported; no path/line drift since the report
  was written.
- `.claude/commands/_audit-validate.sh` passed (exit 0, "OK: all path references valid") —
  no stale-path gate failure for this report.
