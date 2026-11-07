**Source**: `docs/audits/AUDIT_EXTERIOR_2026-11-07.md` (EXT-D3-04).

- **Severity**: LOW (doc rot)
- **Dimension**: Ground-cover pipeline
- **Location**: `docs/engine/exal-groundcover.md` §12.16 ("…12-27 units; widths unchanged"); the `GroundCoverSpecies` doc comment in `crates/core/src/ecs/components/groundcover.rs`
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Status**: NEW

## Description
`4c48e5754` widened the temperate default blade from 1.2–2.35 to 1.7–3.3 BU (and the arid one with it) and raised `GROUNDCOVER_CHROMA_VIBRANCE` to 1.6. `exal-groundcover.md` still says "widths unchanged" and records no vibrance value, and the `GroundCoverSpecies` doc comment in core still describes the earlier retunes. The exterior audit skill's own Dim 3 text already carries the new widths and the 1.6 value.

## Impact
A reader tuning the species trusts the doc over the code.

## Suggested Fix
Update §12.16 and the §8 note in `exal-groundcover.md` (new widths, `GROUNDCOVER_CHROMA_VIBRANCE` 1.6, art direction not sourced); refresh the species doc comment.

## Completeness Checks
- [ ] **SIBLING**: the other places the default blade size is quoted (skill text, `memory-budget.md`, ROADMAP) agree with the code
