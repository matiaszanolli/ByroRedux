# #3678 — PERF-D1-2026-08-30-02: `reemit_water_planes` builds an entity→draw-slot index over **every** draw command each frame with no water-population early-out

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D1-2026-08-30-02`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,water,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3678

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/render/water.rs:111-127`, called unconditionally from `byroredux/src/render/mod.rs:950`
- **Status**: NEW
- **Description**: #3141 correctly replaced an `O(draws × water)` rescan with a single `O(draws)` index build. But the function's only early-out is `world.query::<WaterPlane>()` returning `None`, which happens only when *no entity in the process has ever* carried `WaterPlane` (`World::query`'s contract, `crates/core/src/ecs/world.rs:468-470`). Once an exterior or a water interior has been visited, the storage exists forever; every later frame — including every frame of a dry interior after a door transition, and every frame after a streaming unload emptied the resident water set — still clears, `reserve()`s and re-`extend()`s the map with one entry per draw command, and then iterates zero water planes against it. `QueryRead::is_empty()` already exists (`crates/core/src/ecs/query.rs:79`) and is the exact predicate the function needs.
- **Evidence**:
```rust
// byroredux/src/render/water.rs:111-127
let Some(wq) = world.query::<WaterPlane>() else { return; };   // ONLY guard: storage never created
let mut scratch = world.try_resource_mut::<WaterDrawIndexScratch>();
…
draw_indices.clear();
draw_indices.reserve(draw_commands.len());
draw_indices.extend(
    draw_commands.iter().enumerate().map(|(i, c)| (c.entity_id, i)),
);                                                              // O(all draws), unconditional
let fq = world.query::<WaterFlow>();
let rq = world.query::<RippleEvent>();
for (entity, plane) in wq.iter() { … }                          // may be zero iterations
```
- **Impact**: One `FxHashMap` insert per draw command per frame, thrown away. The repo's own baselines give the draw counts: `bench_draws_cmds` = 3949 (`fo4-InstituteBioScience.tsv`), 2342 (`skyrim_se-WhiterunDragonsreach.tsv`), 2110 (`fnv-FreesideAtomicWrangler.tsv`), 1581 (`fo3-MegatonPlayerHouse.tsv`), 325 (`oblivion-ICMarketDistrictTheGildedCarafe.tsv`) — all five are interiors. **No allocation or timing guard exists for this site**: after warm-up the map keeps its capacity so this is CPU work, not heap churn, and neither `ScratchTelemetry` nor the `cpu_ms:` breakdown brackets `build_render_data`'s water tail. A secondary, smaller observation at the same site: with a *small* live water count the map build can be slower than the scan it replaced (one hash insert per draw vs one integer compare per draw), so #3141's "dozens of surfaces" premise is what makes it a win — that premise is not asserted anywhere.
- **Related**: #3141 (CLOSED, the index that introduced this); `PERF-D2-01` in `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md`.
- **Suggested Fix**: Add `if wq.is_empty() { return; }` immediately after the `wq` acquisition, before the `WaterDrawIndexScratch` resource acquisition and the index build. One line, no behaviour change — `wq.iter()` was already going to yield nothing.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
