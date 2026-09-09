===== ISSUE 3999 =====
REN-2026-09-06-D1-05: four `pub` accessors on `AccelerationManager` have zero call sites, and three of them name a consumer that does not exist — so the deferred-destroy backlog `#3840` introduced is unobservable at runtime
STATE: OPEN
LABELS: bug renderer low memory tech-debt shaders 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-05), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: AS Correctness (observability / dead API)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs`
  (`pending_destroy_blas_count`, `pending_destroy_static_bytes`,
  `pending_destroy_scratch_count`),
  `crates/renderer/src/vulkan/acceleration/memory.rs` (`total_blas_bytes`).
  Ground truth for the named consumers:
  `byroredux/src/commands/mod.rs` (the registry),
  `crates/renderer/src/vulkan/context/mod.rs` (`fill_scratch_telemetry`, what
  `ctx.scratch` actually prints)
- **Status**: NEW (not in the OPEN cache). **Distinct from — and NOT the same
  claim as — the false `REN-2026-08-30-D1-01` / `REN-2026-09-05-D1-02`:
  `integrity_snapshot()` *does* have a live consumer** (see
  `REN-2026-09-06-D1-02`). These four do not.
- **Description**: A `grep` for each accessor across `crates`, `byroredux` and
  `tools` returns only its own definition:
  | Accessor | Docstring claims | Reality |
  |---|---|---|
  | `total_blas_bytes()` | *"reported by `total_blas_bytes()` for telemetry / *tex.stats* console output"* | No caller. There is **no *tex.stats* command** in the registry. Total BLAS VRAM is not surfaced anywhere. |
  | `pending_destroy_blas_count()` | *"Surfaced for `drain_pending_destroys`'s unit test and shutdown telemetry — the count must reach zero after a drain. See #732."* | No caller, and no test names it. |
  | `pending_destroy_scratch_count()` | *"Surfaced for the deferred-destroy regression test and shutdown telemetry … See #1782."* | No caller, and no test names it. |
  | `pending_destroy_static_bytes()` | *"Companion to `pending_destroy_blas_count` for `ctx.scratch` telemetry — the count alone can't show how much VRAM the queue is holding. See #3840."* | No caller. `ctx.scratch` exists, but `fill_scratch_telemetry` emits host-side `Vec`/`HashMap` `(len, capacity)` rows only — no BLAS byte counters. |

  The last row is the sharp one: `fa5c4191` added the accessor **yesterday**
  specifically so an operator could see how much VRAM the deferred-destroy queue
  is holding, and then did not connect it. `live_static_blas_count()` /
  `live_skinned_blas_count()` are the counter-example that shows the wiring
  pattern is available and cheap — they *do* have a consumer
  (`byroredux/src/ownership_sample.rs`).
- **Evidence**:
  - `grep -rn "\btotal_blas_bytes\b" --include='*.rs' crates byroredux tools` →
    definition, field, and doc/comment mentions only; no `total_blas_bytes()`
    call expression anywhere.
  - Same for `pending_destroy_blas_count`, `pending_destroy_scratch_count`,
    `pending_destroy_static_bytes` (the last has field-level reads inside
    `blas_static.rs` itself, but zero calls to the `pub` accessor).
  - `grep -rn '"tex\.stats"' byroredux/src/commands/*.rs` → no match; the
    registered commands in that family are `ctx.scratch` and the memory-frag
    command `mem` + `.frag` (spelled out to avoid reading as a shader path).
  - `crates/renderer/src/vulkan/context/mod.rs`'s `fill_scratch_telemetry` pushes
    `ScratchRow { name, len, capacity, elem_size_bytes }` for
    `gpu_instances_scratch`, `frame_lights_scratch`, `previous_models_scratch`,
    `batches_scratch`, the two rigid-motion maps, `indirect_draws_scratch`,
    `terrain_tile_scratch`, the skin-path sets, and (per #3693) the three
    `tlas_*_scratch` Vecs — all host-side capacities, no device bytes.
- **Impact**: BLAS device residency and the deferred-destroy backlog cannot be
  read from a running engine, so the exact failure `REN-2026-09-06-D1-01`
  describes (a batch overshooting the real budget by the queued amount) is not
  diagnosable in the field even after `#3840` computed the number. Secondary:
  four `pub` items with docstrings that assert consumers which do not exist —
  the same "documented surface that isn't there" pattern that produced the
  *mem.stats* / *tex.stats* family of `REN-LOW L-1` / `L-6` findings.
- **Related**: `#3840` (added the newest of the four), `#732` / `#1782` (the two
  older ones), `REN-LOW L-1` / `L-6` (the *mem.stats* precedent), and
  `REN-2026-09-06-D1-01` (the accounting gap these would have made visible).
- **Suggested Fix**: Add four rows to `fill_rt_integrity_stats`'s neighbour
  (`ScratchTelemetry` is host-only; a `mem.frag`- or `ctx.scratch`-adjacent
  device-side block, or extra `RtIntegrityStats` fields, all work) surfacing
  `total_blas_bytes`, `static_blas_bytes`, `pending_destroy_static_bytes` and
  the two pending counts — and correct the three docstrings so no accessor names
  a command that does not exist. If a consumer is genuinely not wanted for the
  count accessors, delete them rather than leave `pub` items whose docs describe
  a test that was never written.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix

===== ISSUE 4000 =====
REN-2026-09-06-D1-06: `AccelerationManager::skinned_blas` is the last `std::collections::HashMap` on the per-frame skinned path — `#3061` swept every sibling but could not see this one, because the guard test lives in a different file
STATE: OPEN
LABELS: bug renderer low tech-debt 

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

===== ISSUE 4001 =====
REN-2026-09-06-D1-07: `drop_skinned_blas` is the one deferred-destroy push site that bypasses `DEFAULT_COUNTDOWN`
STATE: OPEN
LABELS: bug renderer low 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-07), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: AS Correctness (code quality)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`
  (`drop_skinned_blas`); contract in `crates/renderer/src/deferred_destroy.rs`
  (`DEFAULT_COUNTDOWN`, `DeferredDestroyQueue::push`)
- **Status**: NEW (not in the OPEN cache)
- **Description**: `DeferredDestroyQueue::push`'s doc says *"Production callers
  pass [`DEFAULT_COUNTDOWN`] so the item survives at least
  `MAX_FRAMES_IN_FLIGHT` frames"*, and `DEFAULT_COUNTDOWN` exists precisely so a
  future `MAX_FRAMES_IN_FLIGHT` bump propagates in one place. Three of the four
  push sites into `pending_destroy_blas` / `pending_destroy_scratch` pass
  `DEFAULT_COUNTDOWN`; `drop_skinned_blas` passes `MAX_FRAMES_IN_FLIGHT as u32`
  directly.
- **Evidence**: `blas_skinned.rs`:
  `self.pending_destroy_blas.push(entry, MAX_FRAMES_IN_FLIGHT as u32);` against
  `blas_static.rs`'s `self.pending_destroy_blas.push(entry, DEFAULT_COUNTDOWN);`
  (twice) and `self.pending_destroy_scratch.push(old, DEFAULT_COUNTDOWN);`.
  `crates/renderer/src/deferred_destroy.rs`:
  `pub(crate) const DEFAULT_COUNTDOWN: u32 = crate::vulkan::sync::MAX_FRAMES_IN_FLIGHT as u32;`
- **Impact**: **None behaviourally, today or after any `MAX_FRAMES_IN_FLIGHT`
  bump** — `DEFAULT_COUNTDOWN` *is* `MAX_FRAMES_IN_FLIGHT as u32`, so the two
  expressions are identical by construction and cannot diverge. Reported only
  because the module doc states a convention this site does not follow, and
  because a reader auditing the deferred-destroy contract has to re-derive the
  equivalence at this one site. Filed as the lowest-priority item in this run;
  drop it if the fix budget is tight.
- **Related**: `#372`, `#1449`, `#1782`, `#2481`.
- **Suggested Fix**: One-line substitution to `DEFAULT_COUNTDOWN` (already
  imported in the sibling module; add the `use` in `blas_skinned.rs`).

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

===== ISSUE 4002 =====
REN-2026-09-06-D10-02: `dof_effective_view_proj` applies the lens jitter in ABSOLUTE space, so the aperture offset is quantised away at exterior magnitudes
STATE: OPEN
LABELS: bug renderer low shaders 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D10-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (the DoF path has no production enabler today)
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`dof_effective_view_proj`)
- **Status**: NEW
- **Description**: The function correctly returns a **render-origin-relative** matrix — its
  own doc says so and `look_at_rh(jittered_eye − render_origin, focal_pt − render_origin, up)`
  delivers it. But the two points it subtracts from are *composed* at absolute magnitude
  first: `jittered_eye = pos + lens_u * right + lens_v * up` and
  `focal_pt = pos + focus_dist * fwd`, with `pos` the raw absolute camera position. The
  aperture term is a sub-unit lens offset being added to a value whose f32 ULP, at
  Markarth's X ≈ −176 000, is 0.015625 — so the jitter is quantised (and, below ~0.008 u,
  discarded outright) *before* the rebase that was supposed to protect it. Doing the
  rebase first — `let rel = pos − render_origin;` then composing from `rel` — is exact and
  costs nothing.
- **Evidence**: `jittered_eye` / `focal_pt` are built from `Vec3::from_array(camera_pos)`
  (absolute) and only rebased inside the `look_at_rh` call. The subtraction itself is exact
  (`render_origin` is a multiple of 4096, and the difference is < 4096, hence exactly
  representable) — the loss happens strictly in the addition that precedes it.
- **Impact**: **Currently dormant, and I could not find a way to reach it.** `Camera::aperture`
  defaults to `0.0`, no console command or production code path sets it (the only non-zero
  values in the tree are `2.5` inside `draw.rs`'s own tests), and `fsr_gated_dof` forces
  `aperture = 0.0` whenever FSR — the engine-default upscaler — is active. If DoF is ever
  wired up, a subtle aperture (≲ 0.05 u) would produce ~3 distinct sample offsets instead
  of a smooth disk at exterior worldspaces, i.e. banding rather than bokeh; a large one
  (2.5 u) would still work, coarsely. Reported because it is the one place in the
  render-origin machinery where the rebase happens later than it needs to, and the fix is
  a two-line reorder that removes the question permanently.
- **Related**: #1525 (the degenerate-`focus_dist` guard, intact), #2197 (`fsr_gated_dof`),
  `docs/engine/shader-pipeline.md` §"Render-origin-relative (raster path)".
- **Suggested Fix**: Hoist the rebase: `let rel = Vec3::from_array(camera_pos) - render_origin;`
  then build `jittered_eye_rel = rel + lens_u * right + lens_v * up` and
  `focal_pt_rel = rel + focus_dist * fwd`, passing those to `look_at_rh` directly. Keep
  returning the **absolute** `jittered_eye` (`rel + render_origin`) since the shader's
  view-dir math wants it, as the current doc comment already specifies.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

