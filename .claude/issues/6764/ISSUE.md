# CHAR-2026-11-07-L-05: VatsHitInput::part_to_hit documents a '255 = always' sentinel and BPND@6, relics of the pre-#6448 mis-decode

**Issue**: #6764
**Labels**: documentation, low, game:fnv, game:fo3, character, doc-rot
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-05, Dim 3). Game: FNV, FO3.

## Description
The `VatsHitInput::part_to_hit` field doc says "percent (0-100, 255 = "always"; `BPTD` BPND @6)", a relic of the pre-#6448 mis-decode. Production feeds `part.to_hit_chance as f32` (`BPND`@8) with no 255 handling, and no master carries 255 there. Two tests exercise 255 as if it were a real input.

## Location
`crates/core/src/character/vats.rs:387-388` (field doc), test doc `:553`, `part_to_hit: 255.0` at `:559` and `:594`.

## Evidence
`crates/plugin/src/esm/records/misc/equipment.rs:187-199`: `BPND`@6 is the part's health percent, the to-hit chance is `BPND`@8, 0-100. Byte scan: FalloutNV.esm 304 parts and Fallout3.esm 207 parts, `BPND`@8 in 10..=100, no 255. `systems/vats.rs` `vats_scan`: `part_to_hit: part.to_hit_chance as f32`.

## Impact
Documentation only. A modded part authoring 255 would multiply the chance by 2.55 (capped at `max_chance`), not mean "always".

## Related
#6448 (closed), #6622 (fixed the sibling `weapon_base_to_hit` comment only).

## Suggested Fix
Re-word the field doc to `BPND`@8, 0-100, and drop or relabel the 255 values in the two tests.

## Completeness Checks
- [ ] **SIBLING**: grep the other capture docs, `charal.md`, `feature-matrix.md`, `ROADMAP.md` and code comments for the same stale claim
- [ ] Cite symbols, not line ranges, in the corrected prose
