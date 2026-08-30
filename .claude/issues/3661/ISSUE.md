# #3661 — PERF-D3-2026-08-30-01: `MorphSlot::delta_buffer` holds mesh-static data but is allocated per-entity, with no residency cap, no telemetry, and no `memory-budget.md` row

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D3-2026-08-30-01`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,memory,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3661

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:727-750` (creation),
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:1107-1127` (`flatten_morph_targets`),
  `crates/renderer/src/vulkan/morph_compute.rs:114-166` (`MorphSlot::create`),
  `crates/renderer/src/vulkan/context/mod.rs:1523-1524` (`morph_slots` map)
- **Status**: NEW
- **Description**: `MorphSlot` (#3231, GPU morph-target blending) owns two buffers:
  a DEVICE_LOCAL `delta_buffer` and a host-visible `weight_buffer`. Only
  `weight_buffer` is genuinely per-entity — `MorphSlot::create` writes
  `delta_buffer` exactly once and nothing mutates it afterwards
  (`flush_pending_weights` / `upload_weights` touch only `weight_buffer`,
  `morph_compute.rs:201`). Its contents derive entirely from
  `ImportedMesh::morph_targets`, i.e. from the mesh, not the placement.

  The slot is nevertheless created **per spawned entity**, unconditionally, in
  the per-REFR mesh-spawn path. This is the exact asymmetry the mesh side already
  solves: the same `ImportedMesh` that N REFRs share resolves to **one** refcounted
  GPU mesh via `MeshRegistry::acquire_cached` (`crates/renderer/src/mesh.rs:814`),
  while its morph deltas are copied into VRAM N times.

  Three amplifiers, all verifiable at HEAD:
  1. `flatten_morph_targets` builds a **dense** `target_count × vertex_count`
     array of `[f32; 4]` — 16 B per (target, vertex) — zero-filling every target
     a sparse `NiGeomMorpherController` does not populate, and sizing
     `target_count` as `max(original_index) + 1` rather than the number of
     targets actually present.
  2. There is **no cap** on `morph_slots`. `SkinSlot` has `SKIN_MAX_SLOTS`
     (`crates/renderer/src/vulkan/context/mod.rs:81`, derived from
     `MAX_TOTAL_BONES`); the morph sibling has nothing — creation's only gate is
     `mesh.skin.is_some() && !morph_targets.is_empty()`, as
     `skinned_blas_refit.rs:781` itself notes.
  3. Creation is at **spawn**, not first draw, and `should_evict_skin_slot`
     returns `false` for the `last_used_frame == 0` sentinel
     (`crates/renderer/src/vulkan/skin_compute.rs:238-244`). A placed-but-never-drawn
     morph mesh therefore holds its delta buffer for the whole cell residency;
     only `pending_morph_unload_victims` at cell unload
     (`byroredux/src/cell_loader/unload.rs:291-295`) reclaims it.
- **Evidence**:
  ```rust
  // byroredux/src/cell_loader/spawn/mesh_instance.rs:727-741 — per world.spawn()
  if mesh.skin.is_some() {
      if let Some(morph_targets) = mesh.morph_targets.as_ref().filter(|t| !t.is_empty()) {
          let (deltas, target_count) = flatten_morph_targets(morph_targets, mesh.positions.len());
          match MorphSlot::create(upload_ctx, &deltas, target_count, vertex_count) {
              Ok(slot) => { ctx.morph_slots.insert(entity, slot); }
  ```
  ```rust
  // byroredux/src/cell_loader/spawn/mesh_instance.rs:1116 — dense, not sparse
  let mut deltas = vec![[0.0; 4]; target_count as usize * vertex_count];
  ```
  Per-entity DEVICE_LOCAL cost is exactly
  `target_count × vertex_count × 16 B`. Worked arithmetic (formula, not a
  measurement): a 3 000-vertex head with 20 targets = `20 × 3000 × 16` =
  **960 KB per entity**; ten such placements sharing one model = **9.6 MB**, of
  which 8.64 MB is a byte-identical duplicate of the first.
- **Impact**: VRAM scaling with *placement* count instead of *unique mesh* count
  on exactly the content class (skinned actors, morph-driven props) that is
  densest in the scenes this dimension cares about. Invisible when it happens:
  `grep -i morph` over `docs/engine/memory-budget.md` returns nothing — the
  ledger has no row for it at all — and no console command or `SkinCoverageStats`
  field reports slot count or bytes, so this allocation is unattributable in
  `ctx.scratch`, `mem.frag`, or `skin.coverage`. Bounded (cell unload drains it),
  so a peak problem, not a leak. Same class as the ReSTIR (#1814) and
  SVGF/bloom/caustic (#1872) ledger gaps this doc already documents as findings.
- **Related**: #3231 (feature), #3374 (the eviction sweep that bounds it),
  `SKIN_MAX_SLOTS` precedent. Adjacent-but-out-of-scope observation: the
  `morph_slot.last_used_frame` bump at `skinned_blas_refit.rs:401-403` is nested
  inside the `skin_slots.get_mut(&entity_id)` arm, so an entity with a `MorphSlot`
  but no `SkinSlot` ages out at `min_idle = 3` and, creation being spawn-only,
  never comes back — a correctness question for `/audit-renderer`, not memory
  pressure (it reclaims rather than pressures).
- **Suggested Fix**: Key the delta half by mesh instead of entity — a refcounted
  `FxHashMap<u32 /*mesh_handle*/, Arc<DeltaBuffer>>` mirroring
  `MeshRegistry::acquire_cached`, leaving `weight_buffer` per-entity. Independently,
  add a `morph_slots` row (count + bytes) to `memory-budget.md` and to
  `SkinCoverageStats` so the figure is observable before it is optimised.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
