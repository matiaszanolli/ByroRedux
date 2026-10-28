# PAR-D4-01: No real-data guard pins that vanilla graphs still inline every behavior reference under #6212's budgets. The worst vanilla graph uses 49 of the 64 resolves allowed

Labels: low, import-pipeline, animation, game:fo4, test-gap, bug
Source: docs/audits/AUDIT_PARSERS_2026-10-27.md

**Source**: `docs/audits/AUDIT_PARSERS_2026-10-27.md` finding `PAR-D4-01` (LOW)
**Location (re-mapped to current tree)**: `crates/hkx/src/corpus_tests.rs` (no inline census); code `crates/hkx/src/behavior.rs` `inline_references` / `MAX_INLINED_REFERENCES`

**Severity**: LOW (test-gap). Dimension: Corpus Gates.

## Description
#6212 changed what inlining loads: an ancestor-stack cut, a shared weight budget and a height check before the recursion. Every test of `inline_references` is synthetic. A probe (`fo4inline`) decoded all 328 behaviors in `Fallout4 - Animations.ba2` and inlined each through an archive-backed resolver (same path join as `actor_graph.rs`):
- Today every reference inlines (0 graphs left with an un-inlined reference).
- The worst graph, `meshes\actors\character\behaviors\meleebehavior.hkx`, takes 49 resolves against the cap of 64 (1.3x headroom).

## Impact
A tighter budget (which PAR-D1-01's fix may introduce) or a reference-heavy DLC/Creation graph would silently leave states without their sub-behavior, with no test red.

## Evidence
`crates/hkx/src/corpus_tests.rs` has step and decode censuses (`fallout_4_behavior_graph_census_is_exact`) but no `inline`/`MAX_INLINED` reference (grep at HEAD).

## Related
PAR-D1-01, #6212, #6211.

## Suggested Fix
Add the inline census to `fallout_4_behavior_graph_census_is_exact`: assert every reference resolves and inlines, pin the worst resolve count with a ratio assert against `MAX_INLINED_REFERENCES` and the weight cap.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix

