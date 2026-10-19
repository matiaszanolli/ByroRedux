# FNV-SD-01: Model-less templated CREA shells spawn nothing - 1,877 FNV creature placements (63%) never get a skeleton, mesh, AI or identity

Labels: high,gameplay,ai,bug,game:fnv,legacy-compat

**Severity**: HIGH · **Dimension**: 4/5 (creature spawn at the NPC-spawn boundary) · **Game**: FNV (FO3 shares the path, see Related)

## Description
FNV's wasteland fauna is placed as spawner shells (`VSpawnTier1CoyoteMed`, `VSpawnTier1GiantRatMed50`, `VSpawnSpecialTier3FireGeckoSmallNopah`, `LvlGecko`, ...): `CREA` records with **no `MODL`**, a `TPLT` that points at a `LVLC`, and ACBS template flags with `0x0040` Use Model/Animation set. The engine takes model, skeleton and animations from the leveled pick. This loader resolves every other template category through `ResolvedNpc` but has no Model/Animation terminal: `prepare_creature_state` reads `npc.model_path` of the shell, finds it empty, logs "spawning identity only" at DEBUG, and the Skeleton phase returns `Complete(None)`. The placement root made by `spawn_placement_root` (Transform, Name, ActorValues, FactionRanks, EditorPlacement, creature attack) is left behind with no body, and `finish_actor` (AI package, combat disposition, loot appearance, actor tagging) never runs. The placed REFR gets no `stamp_quest_reference`, no scripts and no `entity_count` increment. The log line is also wrong: nothing is "identity only".

## Evidence
Location: `byroredux/src/npc_spawn/resumable/runtime.rs:445-480` (`prepare_creature_state` derives the skeleton from the shell's own `npc.model_path`), `:543-546` (`if state.skeleton_path.is_empty() { return UnitOutcome::Complete(None); }`), `byroredux/src/npc_spawn/resumable/mod.rs:267-273`, `byroredux/src/cell_loader/references/mod.rs:~765` (`if let Some(root) = result.root { ... }` skips the identity/script/entity-count block), `crates/plugin/src/equip.rs:688-` (`ResolvedNpc` has no `Use Model/Animation` terminal). Re-confirmed present at HEAD.

Census (FalloutNV.esm alone, probes under the audit scratch dir):
- Raw-byte scan of the `CREA` group (no Rust parser): 1,578 records, 1,093 with `MODL`, **435 without `MODL` and with `TPLT`**, 50 with neither.
- All 435 templated model-less `CREA` carry the `0x0040` flag; every one points at a `LVLC` (371 of the lists have at least one directly modelled `CREA` entry, the rest nest).
- Placements of the 435: interior 413 + exterior 1,212 + persistent 252 = **1,877**. Placements of `CREA` bases that do have a model: 1,112. Share without a body: 1,877 / 2,989 = **63%**.
- `walk_inherited_records` already follows `LVLC` (`equip.rs:~600-640`); the pick machinery exists, it is only never asked for the model.
- Disproof attempted: no other path supplies the model (`0x0040` / `USE_MODEL` appears only in a `npc.rs` doc comment); the parser agrees with the raw scan on 435; the marker-model `CREA` (`marker_creature.nif`) are a different set of 33 records; direct placements on `LVLN`/`LVLC` bases: 0. Not verified live (needs the engine).

## Impact
Most of the wasteland's creature population (geckos, bighorners, rats, scorpions, coyotes, ants) is absent in exterior and interior cells. No combat targets, no ambient fauna, no `ACRE` quest creatures that are spawner shells. The 1,877 body-less roots still carry `ActorValues`/`FactionRanks`, a "phantom target for AI and combat scans".

## Related
FNV-SD-04 (the 1,112 that do spawn have no animation), #4137 (open; records `0x0040` as "no consumer yet"), CHAR-SD-03 (same bit), #2567 (closed, built the creature arm), #5495. FO3 twin: FO3-SD-01 (2,641 of 4,397 placed creature refs, 60.1%).

## Suggested Fix
Add a `Use Model/Animation` terminal to `ResolvedNpc` (same `resolve_inherited_record` walk with `0x0040`), and have `prepare_creature_state` derive skeleton/NIFZ/idle/walk from that terminal's `model_path` and `body_part_models`. Decide the LVLC pick policy explicitly (today: highest eligible level, last on a tie). Make the `Complete(None)` branch despawn the orphan root or stamp identity, and fix the log text. Add a test with a model-less shell over a modelled template.

Source: docs/audits/AUDIT_FNV_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)
