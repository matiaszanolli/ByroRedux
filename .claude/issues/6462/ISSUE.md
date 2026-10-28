# FO3-2026-10-27-01: `pack.rs` says 55 of FO3's 56 UseWeapon packages author `PTD2`; 46 do, and the 10 that do not are silently declined

Labels: low, documentation, doc-rot, game:fo3, legacy-compat, ai
Source: docs/audits/AUDIT_FO3_2026-10-27.md

**Source**: `docs/audits/AUDIT_FO3_2026-10-27.md` — finding `FO3-2026-10-27-01` (LOW, doc-rot, Dim 2 ESM slice; consumer `/audit-gameplay`). The doc half is adjacent to open #6375 (feature-matrix / ROADMAP only) and does not overlap it.
**Location (current)**: `crates/plugin/src/esm/records/misc/pack.rs` (the `b"PTD2"` arm comment, ~line 1266-1271); `docs/engine/npc-spawn-ai-packages.md` (line ~139 "`PTD2` ... is **not parsed**", ~354 "`PTD2` (a second target) is not decoded", ~639 "`PTD2` (a second target) still isn't parsed").

## Description
`10f8cbb91` decoded `PTD2` / `PKW3` and wrote in the `PTD2` arm: "Only Use Weapon authors one in vanilla (all 82 FNV and 55 of the 56 FO3 packages)". The FO3 count is wrong: 46 of the 56 FO3 packages author `PTD2`. The engine doc still says `PTD2` is not parsed in three places even though a later paragraph of the same doc (~533-539) describes the decode and correctly says 23 of 56 FO3 packages are install-eligible.

## Evidence
Raw census of `Fallout3.esm` (3,266 PACK): 56 procedure-16 (Use Weapon) packages, all with a 24-byte `PKW3`; `PTDT`/`PTD2` target types: (1,0) x23, (2,0) x22, (0,0) x1, and **no `PTD2` x10**: `FFbaseballPitchingPKG` `0x54457`, `FFbaseballBattingPKG` `0x54454`, `MQ05TaftVictimFiringPKG` `0xB5C4B`, `MS01Mine2..8` `0x2D94D..0x2D953`. `use_weapon_behavior` (`byroredux/src/npc_spawn/ai_package.rs`) does `package.target2?`, so all 10 decline silently ("a target-less package fires into the air"). FNV has no package of this shape.

## Impact
None at runtime beyond the documented decline. The wrong count hides that the "no target" decline is an FO3-only population (10 scripted set pieces) that a future "fire at forward / at nothing" rule would need to handle; the three doc sentences mislead readers about decode status.

## Related
#6375, `10f8cbb91`, #6393 (closed, PACK length gates).

## Suggested Fix
Correct the comment to "46 of the 56 FO3 packages; the other 10 author no target", and strike the three "PTD2 not parsed" sentences in `docs/engine/npc-spawn-ai-packages.md`.

## Completeness Checks
- [ ] **SIBLING**: Other FO3/FNV census numbers in `pack.rs` comments and `npc-spawn-ai-packages.md` (e.g. 23/56 install-eligible) are consistent after the edit
- [ ] **TESTS**: (doc-only) no behaviour change; optionally pin the 46/56 census in an ignored real-data test

