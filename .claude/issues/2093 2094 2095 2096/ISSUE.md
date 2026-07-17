# Issues 2093, 2094, 2095, 2096

All four are from `docs/audits/AUDIT_SKYRIM_2026-07-16.md`, Dimension 3 (NPC Equip + FaceGen, M41).

## #2093 — SKY-D3-NEW-01 (HIGH): Skyrim+ prebaked NPC spawn has no body-mesh fallback
**Location**: `byroredux/src/npc_spawn.rs:1801-1807` (doc comment), `byroredux/src/npc_spawn.rs:1910-1944` (`spawn_prebaked_npc_entity`), `crates/plugin/src/esm/records/actor.rs` (`parse_race` — no `WNAM` handling)

`spawn_prebaked_npc_entity` loads skeleton + FaceGeom NIF + OTFT/CNTO armor only. The FaceGeom NIF is head-only (Bethesda FaceGen convention) — no body/torso/limb geometry. `RaceRecord`/`parse_race` never reads RACE `WNAM` (default skin ARMO). NPCs whose OTFT/CNTO doesn't cover a biped region (e.g. Hulda/Mikael → `0x80` Feet-only) have **zero mesh source** for torso/arms/hands/legs.

**Fix**: Parse RACE `WNAM` into `RaceRecord`, auto-equip resolved skin ARMO as lowest-priority layer before OTFT/CNTO. Fix stale "head and body in one mesh" doc comment.

**Completeness checks**: SIBLING (kf-era `spawn_npc_entity` body-suppression path; all 4 `uses_prebaked_facegen()` games), TESTS (prebaked NPC with only feet-slot armor still has renderable torso/limb geometry).

## #2094 — SKY-D3-NEW-02 (MEDIUM): Slot-displaced armor pieces still render
**Location**: `byroredux/src/npc_spawn.rs:629` (`build_npc_equip_state`, prebaked path — `let _ = equipment_slots.equip(...)`), `byroredux/src/npc_spawn.rs:1331` (kf-era path, displaced indices only `debug!`-logged)

`EquipmentSlots::equip()` returns displaced inventory indices so callers can drop the displaced mesh. Neither spawn path does this. Multi-pick LVLI (bit `0x02`) can produce two entries with overlapping biped bits — both render (z-fight/double-geometry).

**Fix**: After building the full expanded equip list and running every entry through `equipment_slots.equip()`, do a second pass over `armor_to_spawn` dropping any entry whose inventory index no longer appears in `equipment_slots.occupants`.

**Completeness checks**: SIBLING (both line 629 prebaked path AND line 1331 kf-era path need the fix), TESTS (two overlapping-biped-slot CNTO/LVLI entries → only winning mesh renders).

## #2095 — SKY-D3-NEW-03 (LOW): Per-NPC FaceTint DDS resolved but never loaded or applied
**Location**: `byroredux/src/npc_spawn.rs:2011` (`_tint_path` dropped; `prebaked_facegen_tint_path` defined at line 1784)

`prebaked_facegen_tint_path` computes the correct tint texture path but result is dropped (`let _tint_path = ...`), never bound to head material's diffuse slot.

**Fix**: Wire through existing `RefrTextureOverlay` machinery the code comment points at.

**Completeness checks**: TESTS (resolved tint path bound to head material once wired).

## #2096 — SKY-D3-NEW-04 (LOW, docs only): audit-skyrim skill mischaracterizes Dimension-3 skinning-consumer entry point
**Location**: `.claude/commands/audit-skyrim/SKILL.md:134`

Skill names `byroredux/src/systems/character.rs` as "skinning consumer for heads/bodies" — that's actually the player camera/movement controller. Real skinning consumer is `byroredux/src/render/skinned.rs` (already correctly named at line 169 for Dimension 6).

**Fix**: Update Dimension 3 entry-point line to `byroredux/src/render/skinned.rs`. No code path, no test surface.

## Domain classification
- #2093: cross-domain — `byroredux` (binary, npc_spawn.rs) + `byroredux-plugin` (esm, actor.rs WNAM parsing)
- #2094: `byroredux` (binary, npc_spawn.rs only)
- #2095: `byroredux` (binary, npc_spawn.rs only)
- #2096: docs only, no crate
