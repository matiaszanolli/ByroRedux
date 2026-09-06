# #4000 — REN-2026-09-06-D1-06: `AccelerationManager::skinned_blas` is the last `std::collections::HashMap` on the per-frame skinned path — `#3061` swept every sibling but could not see this one, because the guard test lives in a different file

**Labels**: low, renderer, tech-debt, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-06), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: AS Correctness (hot-path hashing residual)
- **Location**: `crates/renderer/src/vulkan/acceleration/mod.rs`
  (`skinned_blas` field declaration and its `HashMap::new()` in
  `AccelerationManager::new`). Probe sites:
  `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances`,
  `self.skinned_blas.get_mut(&draw_cmd.entity_id)` once per skinned draw),
  `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`
  (`has_skinned_blas`, `skinned_blas_entry`, `should_rebuild_skinned_blas`,
  `refit_skinned_blas`, `drop_skinned_blas`)
- **Status**: **Residual of `PERF-D6-01`** (`docs/audits/AUDIT_PERFORMANCE_2026-08-16.md`,
  LOW) — that finding tabulated seven fields; `#3061` converted six of them, all
  of which live in `crates/renderer/src/vulkan/context/mod.rs`. `skinned_blas` is
  the seventh, and the only one outside that file. Not in the 151-issue OPEN
  cache; verified against the code rather than against GitHub, per
  `_audit-common.md`.
- **Description**: `_audit-common.md`'s hot-path rule states the per-frame
  render/skinning path is `FxHashMap`/`FxHashSet` end-to-end *and must stay that
  way across the crate boundary*. `context/mod.rs` now holds that line: eight
  fields are declared `FxHashMap`/`FxHashSet` with `#2923` / `#3045` / `#3061`
  citations, pinned by the `"{what} must stay \`FxHashSet\` (#2923)"` assertions
  and their `FxHashMap` siblings. `skinned_blas` is declared
  `std::collections::HashMap<EntityId, BlasEntry>` and is probed on the same
  per-frame per-entity keyspace: `build_tlas_instances` does one `get_mut` per
  skinned draw command, and the refit path adds `has_skinned_blas` +
  `skinned_blas_entry` + `should_rebuild_skinned_blas` per dirty entity. The
  guard tests cannot catch it because they read `context/mod.rs`'s own source
  text.
- **Evidence**:
  - `crates/renderer/src/vulkan/acceleration/mod.rs`:
    `pub(super) skinned_blas: std::collections::HashMap<EntityId, BlasEntry>,`
    and `skinned_blas: std::collections::HashMap::new(),`.
  - `grep -n "FxHashMap\|FxHashSet" crates/renderer/src/vulkan/context/mod.rs` →
    `rustc_hash` is already imported and used for `skin_slots`, `morph_slots`,
    `morph_delta_cache`, `failed_skin_slots`, `failed_skin_blas`,
    `skin_dispatch_seen_scratch`, `skin_built_this_frame_scratch`,
    `blend_seen_scratch`, `blend_pipeline_cache`, and the two rigid-motion maps.
    `rustc-hash` is therefore already a `crates/renderer` dependency — the change
    is a type substitution with no new dep.
  - `docs/audits/AUDIT_RENDERER_2026-08-30.md` records the sweep as complete:
    *"`pose_dirty_crosses_the_crate_boundary_without_siphash` … covers
    `FrameInputs.pose_dirty`, `record_skinned_blas_refit`'s parameter,
    `skin_slots`, `morph_slots`, `failed_skin_slots`, `failed_skin_blas` and the
    two scratch sets … All green."* `skinned_blas` is absent from that list.
- **Impact**: Small in absolute terms (SipHash-1-3 over a `u32` key, on the order
  of the live skinned-entity count per frame — ~120 on the FO4 baseline). The
  real cost is the one `PERF-D6-01` named: the path now *reads* as Fx-hashed
  end-to-end, and the guard tests say so, while one collection on it is not — so
  the next reader trusts a property that does not hold across the module
  boundary.
- **Related**: `PERF-D6-01` (`docs/audits/AUDIT_PERFORMANCE_2026-08-16.md`),
  `#2923`, `#3045`, `#3061`, `_audit-common.md` §"Hot-path hashing".
- **Suggested Fix**: Change the field to
  `rustc_hash::FxHashMap<EntityId, BlasEntry>` (construct with
  `FxHashMap::default()`), and add a source-shape assertion in
  `crates/renderer/src/vulkan/acceleration/tests/blas_static_tests.rs` mirroring
  the `context/mod.rs` guards so the acceleration module carries its own pin —
  the cross-file blind spot is what let this one survive `#3061`.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
