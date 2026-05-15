# Tech-Debt: Wire StagingPool through scene-load + frame-loop consumers (close #242 long-tail)

**Labels**: renderer, medium, pipeline, tech-debt
**Status**: Open

## Description

Two TODO markers point at the same un-finished consumer-side wiring of [#242](https://github.com/matiaszanolli/ByroRedux/issues/242)'s `StagingPool`. The pool ships and is correct on its own path, but two callers still pass `None` for the pool argument, falling back to per-call create/destroy of the staging buffer.

- `byroredux/src/scene.rs:477` — \`None, // TODO: thread StagingPool through scene load (#242)\`
- `byroredux/src/main.rs:1151` — \`None, // TODO: thread StagingPool through frame loop (#242)\`

Marker age: ~5 months (introduced 2025-12-15 in `2bfb6a2`; #242 closed 2026-01-08 declaring the pool itself done).

## Impact

- **Cell load**: scene.rs path allocates a fresh staging buffer per BSA-texture-decode invocation; on a 1 200-texture interior load that's 1 200 transient `VkBuffer + VkDeviceMemory` pairs the gpu-allocator must service.
- **Frame loop**: main.rs path is the cube-demo / single-mesh entry — only fires once per process, so user-visible impact is "missed performance opportunity," not a frame-rate hit. Still rot.

## Severity rationale

**MEDIUM** (promoted from default LOW). The cell-load case meets the standard severity floor "user-visible performance impact on a documented use case" once profiled: interior load latency in the 100–250 ms range has a measurable fraction attributable to staging-buffer churn (per the #242 issue body's pre-fix profile).

## Proposed fix

Plumb `&mut StagingPool` from `App` through `load_nif_bytes` / `load_nif_from_args` into `scene.rs::load_scene_from_nif` and replace the `None` with `Some(&mut staging_pool)`. Equivalent thread through `main.rs::draw_frame`'s mesh-upload path. Both sites are reachable via the existing function signature — no API redesign required.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: check `byroredux/src/streaming.rs` and `byroredux/src/cell_loader/load.rs` for the same `None,` pattern around BSA decode + mesh upload — if found, fold into this fix
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: `StagingPool` lives on `App` (not behind RwLock); no change
- [ ] **FFI**: n/a
- [ ] **TESTS**: cell-load profile bench should show a delta on interior loads — capture before/after pre-merge

## Dedup notes

Distinct from **#242** (CLOSED — the pool itself ships). This is the consumer-side long-tail. Two open TODOs are the only outstanding work.
Status: Closed (e6192cc)
