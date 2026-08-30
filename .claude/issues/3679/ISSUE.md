# #3679 — PERF-D1-2026-08-30-03: `apply_cell_region_ambient` re-resolves the REGN ambient directive every exterior frame — a `Vec` allocation plus a sort — and both the resource's own doc and the call site's cost comment say it does not

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D1-2026-08-30-03`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,terrain-exterior,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3679

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/app_step.rs:87` (the unguarded call) → `byroredux/src/scene/world_setup.rs:509-523` → `byroredux/src/components.rs:552-575` (`RegionAmbientRes::resolve`) → `crates/plugin/src/esm/records/misc/world.rs:792-804` (`select_active_region_sound`)
- **Status**: NEW
- **Description**: `step_streaming` runs every frame (`byroredux/src/app_events.rs:706`). `apply_cell_region_ambient` is deliberately placed *outside* the `grid_changed` guard (`app_step.rs:82-87`) so a session starting inside a region-tagged cell gets its directive on frame 0. That placement is correct; the cost claim attached to it is not. The comment covering the pair says the unguarded placement "*Costs one map lookup and an `Option<u32>` compare on every other frame*" (`app_step.rs:72-73`) — true of `apply_cell_climate_override`, which early-returns after a couple of map lookups with no allocation, but not of the region call, which runs the full resolve first and only compares afterwards. `select_active_region_sound` **collects a fresh `Vec<&RegionDataEntry>` and sorts it** on every one of those frames. Independently, `RegionAmbientRes`'s own doc block asserts the opposite lifecycle: "*computed once at cell-apply time from data already parsed into `EsmIndex`, **not recomputed per-frame***" (`byroredux/src/components.rs:526-527`). The resource is a `Copy` struct of two `Option<u32>` whose value can only change when the resident grid cell changes.
- **Evidence**:
```rust
// crates/plugin/src/esm/records/misc/world.rs:796-803
let mut candidates: Vec<&RegionDataEntry> = region_form_ids
    .iter()
    .filter_map(|id| regions.get(id))       // std HashMap
    .flat_map(|r| r.entries.iter())
    .filter(|e| e.kind == RegionDataKind::Sound)
    .collect();                             // heap allocation, every frame
candidates.sort_by_key(|entry| std::cmp::Reverse(entry.priority));
candidates.into_iter().next()

// byroredux/src/components.rs:526-527  (the contradicted claim)
/// … computed once at cell-apply time from data already parsed into
/// `EsmIndex`, not recomputed per-frame.
```
- **Impact**: Bounded and small — vanilla exterior cells carry a handful of XCLR regions and the parser's own doc records 788 `RDAT` entries total across `Oblivion.esm` + `FalloutNV.esm` + `Skyrim.esm` (`crates/plugin/src/esm/records/misc/world.rs:827-829`), so `candidates` is short. The allocation is skipped entirely when the cell's regions contribute no `Sound` entry (`collect()` on an empty filter chain does not allocate). The real cost is a malloc/free pair plus a sort per exterior frame for a value that changes only at a cell boundary. **No allocation guard exists for this site**, and the misleading cost comment is what would let the next unguarded per-frame call be added beside it on the same false precedent.
- **Related**: EX-16 item 1 / #2372 (the change that added the call); #2451 / EXAL-03 (the climate sibling the comment actually describes).
- **Suggested Fix**: Cache the resolved `RegionAmbientRes` against the `(worldspace_key, player_grid)` it was computed for — the same shape `applied_climate` already uses for the sibling — and recompute only when that pair moves. Then correct the `app_step.rs:72-73` cost comment to describe both calls, and drop or reword the "not recomputed per-frame" sentence in `RegionAmbientRes`'s doc so it matches whichever behaviour ships.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
