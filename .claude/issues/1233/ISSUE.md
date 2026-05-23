# #1233 — REN-D16-NEW-04: audit-renderer skill references stale test name triangle_frag_dbg_bits_match

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-23_DIM16.md`
**Severity**: LOW
**Dimension**: Audit-skill maintenance / Tangent-Space & Normal Maps

## Symptom

The Dim 16 prompt in `.claude/commands/audit-renderer.md:282` names the lockstep test `crates/renderer/src/shader_constants.rs::tests::triangle_frag_dbg_bits_match`, but the live test is `triangle_frag_dbg_bits_not_redeclared` (renamed post-#1162 / TD4-206).

```bash
$ grep "fn triangle_frag_dbg_bits" crates/renderer/src/shader_constants.rs
190:    fn triangle_frag_dbg_bits_not_redeclared() {
```

`cargo test triangle_frag_dbg_bits_match` returns zero hits.

## Cause

`.claude/commands/_audit-validate.sh` only checks backticked **paths**, not test/symbol names referenced inside backticks via `::` syntax (the skill rule defining what `_audit-validate.sh` enforces lives in `.claude/commands/_audit-common.md`; it explicitly limits the gate to live filesystem refs). So this drift slipped through when #1162 renamed the test as part of TD4-206. Same failure mode as #1229's 4 stale `crates/nif/src/blocks/tri_shape.rs` refs post-#1118 split — symbol/path renames in production code leave audit skills stale.

## Impact

An auditor checking the claim runs `cargo test triangle_frag_dbg_bits_match`, gets zero results, and either (a) believes the lockstep is missing → files a false-positive finding, or (b) digs through the file to find the real test name. Either path wastes auditor time. The current Dim 16 audit (`AUDIT_RENDERER_2026-05-23_DIM16.md`) was the first to catch this drift precisely because it walked the test by name.

## Fix

Update the audit-skill text at `.claude/commands/audit-renderer.md:282` to name both real tests and describe their division of labour:

- `triangle_frag_dbg_bits_not_redeclared` — negative-side check (no `const uint` redeclaration in `triangle.frag`).
- `generated_header_contains_all_defines` — positive-side check (each `#define` is emitted by `build.rs` with the correct value).

The Rust source-of-truth list itself is at `crates/renderer/src/shader_constants_data.rs::DBG_*` (already correctly named in the prompt).

## Optional structural follow-up

Consider extending `_audit-validate.sh` to validate referenced test/symbol names (something like: parse backticked refs containing `::tests::` or `::fn_name` and grep for them in the codebase). Defer behind #1229 — if that issue lands a structural fix for the path-drift class, the same fix can probably absorb the symbol-drift class.

## Regression Risk

NONE — pure documentation fix.

## Completeness Checks

- [ ] **SIBLING**: scan other audit skill files for the obsolete symbol `triangle_frag_dbg_bits_match` (likely none, but verify via `grep -rn "triangle_frag_dbg_bits_match" .claude/`)
- [ ] **SIBLING**: scan for other `::tests::` symbol-backticked refs in audit skills that may have drifted (sample: `grep -rn "::tests::" .claude/commands/audit-*.md`)
- [ ] **TESTS**: rerun `.claude/commands/_audit-validate.sh` to confirm no regression on path-side
