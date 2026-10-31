# ESM-2026-10-31-03: Stale "no runtime consumer" and "minimal stub" annotations (EXPL, COBJ, MSET) and a few related prose slips

Labels: low,esm-plugin,documentation,doc-rot

Source: `docs/audits/AUDIT_ESM_2026-10-31.md` (ESM-2026-10-31-03). Severity LOW (doc-rot). Same class as 10-27 ESM-04.

**Location**: `crates/plugin/src/esm/records/index.rs:546` (`explosions`) and `:574` (`recipes`); `docs/engine/esm-records.md:433-446` and `:449-459`; `crates/plugin/src/esm/records/misc/pack.rs:1144`

## Description
- (a) `explosions` and `recipes` carry "Parsed and retained, no runtime consumer yet (#6180)". `byroredux/src/systems/mines.rs:72` now reads `index.explosions`; `byroredux/src/crafting.rs:95` reads `index.recipes` (`CraftingBook::from_index`). `esm-records.md:433-446` still lists both in the "seven typed maps, nothing outside `records/` reads them" paragraph, and its own re-check grep would now find them.
- (b) `esm-records.md:449-459` says 29 record types remain minimal stubs and lists MSET, MUSC, GRAS, RADS, CHAL, CMNY, CCRD, LSCR. MSET became typed this window (`parse_mset`, `9ee802d57`); the others were typed earlier (`parse_musc`, `parse_gras`, `parse_rads`, `parse_chal`, items, `parse_lscr`). The "~90 categories" row at `:54` is also stale (114 pinned).
- (c) `pack.rs:1144` says "FO3/FNV's 8-byte PKDT has no word there". Measured FNV `PKDT` is 12 bytes; the 8-byte form is Oblivion's. The type-byte 18/19 gate keeps the code correct.

## Impact
Misleading to dead-code sweeps and the next audit. No behaviour change.

## Suggested Fix
Drop `explosions` and `recipes` from the unread-map annotations (and the paragraph and its grep), refresh the stub list from the dispatch table, correct the `PKDT` comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
