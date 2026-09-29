# #5123: RT-2026-09-29-02: Oblivion light_count_directional 2 → 6 — the player body re-spawns the `__max_default_light` exporter-artifact pair (#3557 de-dup does not collapse it)

**Labels**: high, bug, renderer, game:oblivion

**Source report**: `docs/audits/AUDIT_RUNTIME_2026-09-29.md` (report ID `RT-2`)
**Severity**: HIGH (rendering correctness: four extra full-intensity directional lights reach the GPU light buffer)
**Dimension**: Telemetry diff (exact metric)

## Location
- Baseline row: `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv` `light_count_directional 2`
- `byroredux/src/player_body.rs` (`attach_player_body`, added in `a070baaad`) → `NpcSpawnJob` → `load_nif_bytes_with_skeleton` (`byroredux/src/scene/nif_loader.rs`, the `spawn_nif_lights` call)
- `byroredux/src/cell_loader/spawn.rs` `spawn_nif_lights` (the #3557 name de-dup: `is_known_exporter_artifact_light_name(..) && world.find_by_name(..)`)

## Description
Game / cell / baseline / current: oblivion / ICMarketDistrictTheGildedCarafe / 2 / 6.

`light.dump` at HEAD lists six `kind=Directional source=nif/synthetic` emitters, all named `__max_default_light`, as three ± pairs:
- 120/121: the baseline's NPC pair.
- 839/840 and 855/856: new.

Entity 839's direction `[-0.4467,0.7444,0.4963]` and radiance are byte-identical to entity 120's. That is exactly the duplicate #3557 (RT-11, CLOSED) was meant to suppress with `is_known_exporter_artifact_light_name(..) && world.find_by_name(..)`. Bisected: `69266fb82` (= `a070baaad^`) has dir 2 and `lights=10`; `a070baaad` has dir 6 and `lights=14`. The bench line's `lights=` is `gpu_lights.len()` (`byroredux/src/bench.rs`), so the four extras are submitted to the GPU. `HiddenFirstPerson` hides the body's meshes but not the lights under them.

## Evidence
`/tmp/audit/runtime/oblivion-ICMarketDistrictTheGildedCarafe.telem.txt` (`LightSource emitters: 14`); bisect table in the report. `state_hash` at HEAD equals `a070baaad`'s.

## Impact
Every Oblivion cell with the player character gains two white directional lights per body NIF that carries the exporter artifact. They add unauthored fill light, including in first person where the body itself is invisible. The fact that the pre-existing NPC pair 120/121 also survives suggests the #3557 de-dup is not effective on the NPC/loose-NIF path at all — a wider question than this commit.

## Related
#3557 (closed; de-dup exists but does not collapse these), #4938 (different LIGH-falloff issue), `a070baaad`.

## Suggested Fix
Find out why `find_by_name("__max_default_light")` does not match on the `load_nif_bytes_with_skeleton` path (pool lookup, `Name` timing, or storage). Either fix the de-dup there, or skip exporter-artifact lights for actor-part NIFs altogether, since actor bodies should not emit scene lights. Pin it with a test that assembles an actor with the artifact twice and asserts one emitter.

Validated at HEAD 9fcfdc3fc: the #3557 de-dup branch is unchanged in `spawn_nif_lights`; `load_nif_bytes_with_skeleton` still calls it with identity placement; player body assembles via `NpcSpawnJob`; runtime evidence is from the audit capture at this same HEAD.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (NPC spawn, loose NIF, cell REFR light paths)
- [ ] **TESTS**: A regression test pins this specific fix
