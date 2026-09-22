# PAR-D5-2026-09-21-05: UVD docs disagree, including a stale skill line and a field doc that contradicts its own module

Labels: low,documentation,import-pipeline,doc-rot,game:fo4

## Description
`crates/bsa/src/uvd.rs:157-164`: `UvdHeader::bounds_min`'s doc comment says the box is "Not quantised ... a tight content bound, not a grid-aligned cell volume." The module doc (`:79-101`, dated 2026-09-15) and `exterior_cell_grid()` in the same file establish that exteriors **are** a grid-aligned 3x3 block (1,095/1,095 measured) — the field doc contradicts the module doc it sits inside.

Separately, `byroredux/src/cell_loader/precombined.rs:50-73` says no consumer exists for UVD yet, and `parse_uvd_header` has no non-example caller (`grep -rn parse_uvd_header byroredux/src crates` shows only `crates/bsa/examples/probe_uvd_corpus.rs`). Yet `.claude/commands/audit-parsers/SKILL.md` (Dim 5) still says "UVD: envelope only, consumed in `cell_loader/precombined.rs`" — also stale.

Verified unchanged at HEAD `ee6d3fb39`: `bounds_min`'s doc comment still reads the same contradicting text.

## Evidence
`grep -rn parse_uvd_header byroredux/src crates` returns only the example probe, confirming no production consumer exists yet.

## Impact
Misleading docs for the future previs consumer (#3810) and for the next audit run reading this field's contract.

## Related
#3810 (the UVD-consumer research spike this format feeds)

## Suggested Fix
Fix the field doc to match the module doc (exterior vs interior quantisation). Correct the skill bullet to "envelope only, no consumer yet".

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D5-2026-09-21-05)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix