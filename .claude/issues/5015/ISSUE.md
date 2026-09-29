# #5015: SKY-D4-2026-09-29-01: Skyrim/FO4 authored corpse poses (XRGD) are never decoded — 1,445 Skyrim + 1,418 FO4 Starts Dead actors ragdoll-collapse from their spawn pose instead of lying where authored

**Labels**: bug, medium, legacy-compat, game:fo4, game:skyrim, esm-plugin, physics

**Source report**: `docs/audits/AUDIT_SKYRIM_2026-09-29.md` (FO4 scope added from `docs/audits/AUDIT_FO4_2026-09-29.md` Dim 4)
**Severity**: MEDIUM
**Dimension**: TES5 cell load (Skyrim/FO4 data through #4814)

## Location
- `crates/plugin/src/esm/cell/walkers.rs` (`PlacedRef` construction): only the header bit is decoded; there is no `XRGD` arm.
- `byroredux/src/cell_loader/reference_state.rs` (`apply_starts_dead`).
- `byroredux/src/combat.rs` (`reconcile_dead_actor` → `activate_ragdoll`, seeded from the current bone globals).

## Description
Skyrim (and FO4) author a placed corpse as ACHR `0x200` plus an `XRGD` ragdoll pose (xEdit `wbRagdoll`, 28 bytes per bone entry). Only the bit is decoded. `apply_starts_dead` inserts `Dead` and queues reconciliation; `reconcile_dead_actor` then builds the multibody from the freshly spawned skeleton's bind/idle pose. `XRGD` has 0 readers in `crates/` / `byroredux/`.

Every authored corpse therefore starts upright and falls, instead of lying slumped, draped over a table, hanging or impaled as authored.

This is the **pose** half of the corpse problem. On Skyrim/FO4 the dead **marker** is correct (#4814); on FO3/FNV `XRGD` is also the only marker (#5005), so one `XRGD` decode serves both issues.

## Evidence
Byte census of the installed masters (24-byte TES5 headers; compressed bodies inflated):

| Plugin | Starts Dead ACHRs | …with `XRGD` |
|---|---|---|
| `Skyrim.esm` | 1,151 | 1,097 |
| `Dawnguard.esm` | 144 | 140 |
| `Dragonborn.esm` | 212 | 208 |
| `Fallout4.esm` | 1,188 | 1,090 |
| `DLCRobot.esm` | 14 | 14 |
| `DLCCoast.esm` | 165 | 162 |
| `DLCNukaWorld.esm` | 164 | 152 |

- Skyrim total 1,445; FO4 total 1,418. Every `XRGD` length is a multiple of 28.
- No non-Starts-Dead ACHR carries `XRGD` on either game (FO4: 0 of 8,791 live ACHRs).
- FO4 also sets bit 29 "Don't Havok Settle" on 124 refs, all of them corpses (`wbDefinitionsFO4.pas`), which is part of the same authored-pose intent.

Validated at HEAD 9fcfdc3fc: `rg XRGD crates byroredux` returns 0 hits; `apply_starts_dead` only inserts `Dead` and queues `queue_dead_actor_reconciliation`.

## Impact
Every vanilla authored corpse on Skyrim and FO4 loses its placement. Bodies on ledges, tables or gibbets can move or fall, which displaces quest-clue and loot containers. It persists across save/load, because the ragdoll re-seeds the same way.

## Related
- #4814 (CLOSED), #5005 (FNV-2026-09-29-D2-01: `XRGD` as the FO3/FNV dead marker), PHYSAL `activate_ragdoll`.

## Suggested Fix
- Decode `XRGD` into `PlacedRef` as a per-bone `(bone_id, position, rotation)` list, cited to xEdit `wbRagdoll`.
- Pose the skeleton from it before `activate_ragdoll`, or spawn the corpse asleep in that pose. Honour FO4's "Don't Havok Settle" (bit 29) as "keep asleep in the authored pose".
- The same decode serves the FO3/FNV marker fix (#5005).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (Skyrim, FO4, and the FO3/FNV/Oblivion corpse paths that also carry `XRGD`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes in the ragdoll activation path, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix (synthetic `XRGD` decode + posed-ragdoll seed)

## Cross-references: the starts-dead / starts-unconscious decode family (2026-09-29 suite)
#4814 only knows the Skyrim/FO4 ACHR header bit. Each game below has a distinct marker, so each is filed separately:
- #5005 — FO3/FNV: `XRGD` presence is the only corpse marker (rule **inferred from data, needs a source before fixing**).
- #5013 — Oblivion: base `NPC_`/`CREA` header flag `0x80000` + 0 Health (sourced: xEdit TES4, CS wiki).
- #5015 — Skyrim/FO4: the `XRGD` corpse pose is never decoded (marker is correct; pose is lost).
- #5017 — FO4: ACHR `0x2000` "Starts Unconscious" undecoded (dormant semantics **inferred, needs a source before fixing**).

