# ESM-2026-10-27-04: Regression of #5830: stale LCTN dispatch prose never fixed, plus stale `body_parts` "no consumer" annotation

Labels: low, esm-plugin, documentation, doc-rot
Source: docs/audits/AUDIT_ESM_2026-10-27.md

**Report**: `docs/audits/AUDIT_ESM_2026-10-27.md` · **Finding**: ESM-2026-10-27-04 · **Severity**: LOW (doc-rot) · **Dimension**: Record Schema Dispatch
**Status**: Regression of #5830 (closed, fix never applied) + NEW (`body_parts` annotation)
**Location**: `crates/plugin/src/esm/records/parse.rs:33-39`; `crates/plugin/src/esm/records/index.rs:512-516`; `docs/engine/esm-records.md:440`

## Description
(a) The `DISPATCH_HANDLED_FOURCCS` doc says "LCTN has never had a top-level dispatch arm ... re-adding LCTN to the list would over-report coverage". LCTN is in the list (`parse.rs:57`) and routed by the misc-stub arm (`parse.rs:445`). #5830 filed exactly this and was closed on 2026-10-15 by `af7cd286b`, but that commit touches only `ci.yml`, `CLAUDE.md`, `ROADMAP.md` and two smoke docs, not `parse.rs`.

(b) `EsmIndex::body_parts` still reads "Parsed and retained, no runtime consumer yet (#6180)" and "per-NPC dismemberment routing + biped slot count". `esm-records.md:440` lists `body_parts` among the maps with no readers. Since #6350, `byroredux/src/systems/vats.rs::resolve_limb_table` reads it, and `BptdRecord` no longer has a slot count.

## Evidence
Re-read at HEAD `fa6be4a6b`: the `parse.rs:33-39` doc block and `index.rs:512-516` annotation are unchanged; `git log -- crates/plugin/src/esm/records/parse.rs` shows no edit since #5830 closed.

## Impact
The (a) note tells a maintainer to remove a correct list entry, the inverted regression it warns about. The (b) annotations mislead dead-code sweeps.

## Related
#5830 (closed, regression), #6180, #6350.

## Suggested Fix
- Apply #5830's intended wording to `parse.rs`.
- Drop `body_parts` from the unread-map annotations in `index.rs` and `esm-records.md`, and update the field doc.

## Completeness Checks
- [ ] **SIBLING**: Other "no runtime consumer yet" annotations (#6180 maps) re-checked against current readers
- [ ] **TESTS**: `dispatch_handled_fourccs_matches_the_live_dispatch_arms` still green

