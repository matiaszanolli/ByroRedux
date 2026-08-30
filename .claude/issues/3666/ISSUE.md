# #3666 — PERF-D5-2026-08-30-01: the TLAS UPDATE gate compares an *ordered* BLAS-address sequence produced by the per-frame draw sort, so ordinary frustum churn forces a full BUILD

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D5-2026-08-30-01`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,pipeline,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3666

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs:218-228`
  (`decide_use_update`); `crates/renderer/src/vulkan/acceleration/tlas.rs:99-113`
  and `:445-463` (`build_tlas_instances`); producer
  `byroredux/src/render/mod.rs:546-572` (`sort_draw_commands`) and `:407-533`
  (`draw_sort_key`)
- **Status**: NEW
- **Description**: `decide_use_update` picks UPDATE (cheap refit) only when the
  current frame's BLAS device-address list is **element-for-element equal, in
  order**, to the list captured at the last BUILD. That list is materialised by
  `build_tlas_instances`, which walks `draw_commands` in exactly the order
  `sort_draw_commands` left them. That order is not frame-stable:
  * `sort_draw_commands` first runs an **unstable** in-place partition that hoists
    `in_raster` draws to the front. A single entity crossing the frustum boundary
    both moves the raster/RT-only boundary *and* re-permutes the RT-only tail
    (the tail order is a side effect of the swap sequence, and the function
    deliberately does not sort it).
  * The raster prefix is then fully re-sorted. In the opaque arm `mesh_handle`
    outranks `sort_depth`, so cross-mesh opaque order is stable — but the
    alpha-over arm is **depth-primary** (`!cmd.sort_depth` sits at slot 4, above
    `mesh_handle`), by deliberate design for the `#1804`/`#2237` compositing fix.
    `sort_depth` is `f32_sortable_u32(clip.w)` — full precision, unquantised — so
    any two TLAS-eligible transparents of different meshes swap places the moment
    the camera crosses their bisector.

  Neither churn source bumps `blas_map_generation`, so `decide_use_update` reaches
  its `layout_matches` zip-compare, fails it, and returns BUILD. The comparison
  itself is O(N) and, in this regime, is O(N) work spent proving that a BUILD is
  needed.
- **Evidence**: `predicates.rs:223-228`
  ```rust
  let layout_matches = cached_addresses.len() == current_addresses.len()
      && cached_addresses
          .iter()
          .zip(current_addresses.iter())
          .all(|(a, b)| a == b);
  (layout_matches, true)
  ```
  `tlas.rs:445-463` builds `instances` by iterating `draw_commands` in the sorted
  order and pushes `acceleration_structure_reference` per surviving entry;
  `mod.rs:546-562` is the unstable partition (`draw_commands.swap(raster_len, index)`)
  whose doc comment states outright that the tail is left unsorted.
- **Impact**: The engine's own instrumentation states the intent this defeats —
  `GpuTimerSnapshot::tlas_build_ms`'s doc comment
  (`crates/renderer/src/vulkan/gpu_timers.rs:141-144`) says "First-cell-load frames
  spike (full BUILD); steady-state should report an UPDATE-mode refit in the
  sub-millisecond range." Under camera motion — i.e. all of normal play — the
  address permutation changes and the refit path is not taken. Blast radius is
  every RT frame on every game; magnitude scales with TLAS instance count, so
  dense FO4/Skyrim city cells pay most. There is also **no build-vs-update counter
  anywhere in the telemetry surface** (`memory.rs` exposes only sizes), so the
  only current way to see this is `tlas_build_ms` — which is exactly why it has
  gone unnoticed.
- **Related**: `AUDIT_PERFORMANCE_2026-05-10.md:175` recorded "Static cells = REFIT
  every frame after the first. **Confirmation, not a finding.**" — that observation
  was made against a *static* camera and predates the depth-primary alpha arm
  (`RT-1`/`#2215` note in `draw_sort_key`) and `#2682`'s partition rework. Open
  `#2367` (FO4 ~33–34% slower) is a plausible but unproven consumer of this; it is
  **not** claimed here.
- **Suggested Fix**: TLAS instance *order* is semantically irrelevant — ray hits
  resolve through `instance_custom_index`, which `build_instance_map` sets per
  instance, so nothing downstream reads position. Either (a) emit TLAS instances in
  a frame-stable order (e.g. sorted by BLAS device address, or by `entity_id`)
  independent of the raster sort, or (b) make `decide_use_update` order-independent
  by comparing a commutative digest of the address multiset. (a) is preferable
  because it also stabilises the cached list. Both are pure-CPU changes testable
  against the existing `decide_use_update` unit tests; confirm the win with
  `tlas_build_ms` before and after. Flag: driver BVH quality can depend weakly on
  instance ordering, so the A/B should also watch `main_render_ms`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
