# Renderer Audit — Dimension 12 (GPU Skinning Compute + BLAS Refit)

**Date**: 2026-05-28
**Scope**: `/audit-renderer --focus 12 --depth deep` — GPU skinning compute (M29.5 palette pre-dispatch + M29.3 pre-skinning) and per-skinned-entity BLAS UPDATE-mode refit.
**Method**: 3 finder lenses (skinning-compute / BLAS-refit+barriers / bone-palette+lifecycle) → cross-lens dedup → adversarial per-finding verification against current code (default-refute). 7 candidates → 6 reported (2 merged); 0 refuted. ~50 invariants verified to hold (see coverage record).

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 3 |
| INFO | 1 |

The skinning→BLAS pipeline is in good shape: every barrier in the COMPUTE→AS-BUILD→FRAGMENT chain, the VUID-03667 build/refit flag+count pairing, output-buffer usage flags, the M36-LRU skinned-BLAS pin, scratch alignment, and the pose-dirty gate all verified correct. The one correctness-relevant finding is **D12B-1**: a missing scratch-serialize barrier before the *first* skinned BLAS build of a batch when it reuses the shared scratch buffer after a separately-submitted cell-load BLAS batch. The remaining findings are a latent (currently-unreachable) defensive gap, a telemetry-semantics mislabel that could mislead the #1284 cap-sizing loop, a test gap, and count doc-rot.

No CRITICAL/HIGH. No fix is release-blocking; D12B-1 should be fixed before any change that interleaves skinned-BLAS builds with cell-load BLAS builds in the same frame.

## Findings

### MEDIUM

#### D12B-1: First-sight skinned BLAS BUILD (i==0) reuses the shared scratch buffer with no AS_WRITE→AS_WRITE serialize barrier

- **Dimension**: GPU Skinning / Acceleration Structures
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:242-266` (build loop — `record_scratch_serialize_barrier` is gated `if i > 0`); cross-submission predecessor `build_blas_batched` (cell-load static BLAS) shares the same `blas_scratch_buffer`.
- **Issue**: `build_skinned_blas_batched_on_cmd` sizes the shared `blas_scratch_buffer` once for the batch max, then records each build with an `AS_WRITE→AS_WRITE` serialize barrier **only for `i > 0`**. The `i == 0` build has no preceding barrier on the shared scratch. When a cell-load static-BLAS batch (`build_blas_batched`) ran earlier and wrote the same scratch buffer — even in a prior submission — the first skinned build can begin reading/writing scratch before the prior build's scratch writes are visible. The refit path already self-emits this barrier as its first statement (the #983 pattern); the batched-build path does not.
- **Risk**: Scratch-buffer WAR/WAW hazard across builds sharing one scratch allocation. Manifests as intermittent BLAS corruption (garbage triangles / dropped geometry on the first skinned mesh of a cell) under driver scheduling that overlaps the two builds. Not a guaranteed fault — depends on timing — which is why it has not surfaced in the steady-state bench.
- **Suggested Fix**: Self-emit `self.record_scratch_serialize_barrier(device, cmd)` once at the top of the Phase-3 record loop in `build_skinned_blas_batched_on_cmd`, before the `i == 0` build (mirroring `refit_skinned_blas`'s self-emit). It is idempotent with the existing `i > 0` barriers (the first becomes redundant-but-harmless, or gate the in-loop one to `i > 0` as today and add the single pre-loop one).
- **Dedup**: NEW.

#### C-1: `overflow_attempt_count` is a per-call cumulative counter, not the per-frame distinct-entity demand the #1284 cap-sizing loop reads it as

- **Dimension**: GPU Skinning — telemetry semantics
- **Location**: `crates/core/src/ecs/resources.rs:738-739` (increment), `:685-690` (field doc), `:777-783` (accessor); consumed at `crates/renderer/src/vulkan/scene_buffer/constants.rs` (cap-sizing comment) + `DebugStats::skin_pool_overflow_attempts`.
- **Issue**: `overflow_attempt_count` increments on **every** over-cap `allocate()` call and is **never reset**. Because over-cap entities are never inserted into `entity_to_slot`, a single persistently-overflowing entity re-increments the counter every frame. The field docstring (`resources.rs:685`) describes it as "distinct entities," and the #1284 cap-sizing comment treats it as per-frame over-demand — but it is a monotonic session-cumulative count of over-cap *calls*. Reading it as "how many slots short are we this frame" overshoots by the frame count.
- **Risk**: The #1284 cap-sizing feedback loop (size the pool from observed spill) would over-size the pool if it trusts this number as per-frame demand. Telemetry is misleading, not corrupting.
- **Suggested Fix**: Either (a) relabel the field/accessor/comment to state plainly it is a monotonic cumulative count of over-cap `allocate()` calls (and stop describing it as per-frame / distinct-entity), or (b) if a per-frame distinct-entity demand signal is actually wanted by the cap-sizing loop, track a separate per-frame `HashSet<EntityId>`-backed high-water and reset it each frame.
- **Dedup**: NEW. (Adjacent to #1284's instrumentation; the mislabel was introduced with it.)

### LOW

#### DIM12-A-01: `SkinSlot.vertex_count` is never reconciled against the per-frame `mesh.vertex_count` — latent OOB compute write on an entity→mesh remap

- **Dimension**: GPU Skinning — compute dispatch
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:913-980,1014-1053`; `crates/renderer/src/vulkan/skin_compute.rs:74-77` (doc invariant), `:116-118` (dead `vertex_count()` accessor).
- **Issue**: The first-sight loop calls `create_slot` only when the slot is absent (`!self.skin_slots.contains_key(&entity_id)`); an existing slot is reused verbatim, with no comparison of the live `mesh.vertex_count` against `slot.vertex_count`. The dispatch pushes `push.vertex_count = mesh.vertex_count` into the existing slot's `output_buffer` (sized `vertex_count_at_alloc × VERTEX_STRIDE_BYTES`), and `skin_vertices.comp` writes `outputVertexData[vid * VERTEX_STRIDE_FLOATS …]` for `vid in 0..push.vertex_count`. If an entity's `mesh_handle` is remapped to a larger-vertex-count mesh, the write runs past `output_size`. The shader's bounds check gates on `push.vertex_count`, not the slot's allocated capacity, so it does not protect against this. `SkinSlot::vertex_count()` exists but has zero callers.
- **Risk**: Out-of-bounds compute write into the skinned-vertex SSBO. **Not reachable in current code** — per the #907 comment (`blas_skinned.rs:383-385`) no in-engine path remaps `entity_id → mesh` between frames, and a vertex-count change requires a fresh `mesh_handle`. Latent/defensive: the BLAS side already guards this exact remap via `validate_refit_counts` (drops the BLAS on mismatch), but the compute dispatch runs *before* the refit guard, so the protection is asymmetric.
- **Suggested Fix**: In the first-sight loop, when the slot exists, compare `slot.vertex_count() != vertex_count` and on mismatch `destroy_slot` + recreate (and drop the skinned BLAS so first-sight rebuilds). This makes the `SkinSlot` doc invariant load-bearing and activates the dead `vertex_count()` accessor — symmetric with `validate_refit_counts`.
- **Dedup**: NEW.

#### D12B-2: `BlasEntry::built_flags` doc references a nonexistent predicate `validate_refit_inputs`

- **Dimension**: Acceleration Structures — doc rot
- **Location**: `crates/renderer/src/vulkan/acceleration/types.rs:56-59`.
- **Issue**: The `built_flags` doc comment cites `validate_refit_inputs`; the actual predicate is `validate_refit_flags` (paired with `validate_refit_counts`). No such `validate_refit_inputs` symbol exists.
- **Risk**: Doc rot. No correctness impact.
- **Suggested Fix**: Replace `validate_refit_inputs` with `validate_refit_flags` at `types.rs:58`.
- **Dedup**: NEW.

#### C-2: No test asserts the bone-palette / SkinSlotPool overflow guard fires

- **Dimension**: GPU Skinning — test coverage
- **Location**: `byroredux/src/render/bone_palette_overflow_tests.rs:85-107` (over-capacity test exercises clamp/None but not the latch); `crates/core/src/ecs/resources.rs:944-957` (`returns_none_at_max_skinned`).
- **Issue**: The overflow guard (one-shot `overflow_warned` latch + `overflow_attempt_count` increment + `allocate()` returning `None` past `max_slot`) is verified by this audit to work, but no test asserts the *latch fires* / the *counter increments*. The historical M29 regression was silent truncation past the cap; the guard preventing it is unpinned.
- **Risk**: Test gap. A future refactor could drop the warn/counter increment without failing CI.
- **Suggested Fix**: Add a `SkinSlotPool` unit test (in `skin_slot_pool_tests`) that allocates `max_slot + K` entities across `K` over-cap calls and asserts `overflow_attempt_count() == K` and that a subsequent `allocate()` still returns `None` without panic.
- **Dedup**: NEW.

### INFO

#### DIM12-DOC-1: Bone-palette / SkinSlotPool capacity comments are off-by-one (1365 vs 1364) + carry stale 32768-era figures

- **Dimension**: GPU Skinning — doc rot (merges candidates DIM12-A-02 + C-3)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:41,66,73`; `byroredux/src/main.rs:873-874`; `crates/renderer/src/vulkan/scene_buffer/constants.rs:15,20-21,40,45`; `crates/renderer/src/vulkan/skin_compute.rs:319-323`; `byroredux/src/render/bone_palette_overflow_tests.rs:76`.
- **Issue**: `SKIN_MAX_SLOTS = (MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1 = (196608 / 144) - 1 = 1365 - 1 = **1364**` (slot 0 reserved → 1364 allocatable), and the descriptor pool is `1364 × 2 × 3 = **8184**`. Multiple comments state `1365` and `8190` (they omit the `-1`). Separately, `bone_palette_overflow_tests.rs:76` and `skin_compute.rs:319-323` still carry `floor(32768/144) = 227` / `× 144 = 32688` figures from the pre-#1284 `MAX_TOTAL_BONES = 32768` era (now 196608 → `196560`). One adjacent value note: `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME = 1366` is a harmless over-provision (`≥` the 1364 capacity, verified safe) but its comment `(= 196608/144)` reads as 1365 and is labeled as matching the slot capacity — off by 1–2 in the annotation only.
- **Risk**: None. The code is consistent (every consumer derives from the single `SKIN_MAX_SLOTS` expression — verified); only the human-readable annotations drifted. `constants.rs:64-86` is a deliberate HISTORY-LOG of the cap evolution and should be left as-is (not "fixed").
- **Suggested Fix**: Update the off-by-one annotations to `1364 / 8184`; refresh the 32768-era figures (`227`→`1365`, `32688`→`196560`); correct the `MAX_PENDING` comment to `196608/144 = 1365` and note it over-provisions the 1364-slot capacity by design. Leave the history-log block intact. Fold into the next touch of these files.
- **Dedup**: NEW (annotation rot introduced incrementally by #1284's cap triple-bump).

## Prioritized Fix Order

1. **D12B-1** (MEDIUM, sync correctness) — add the pre-loop scratch-serialize barrier in `build_skinned_blas_batched_on_cmd`. Small, surgical, prevents an intermittent first-skinned-mesh BLAS corruption when cell-load + skinned builds share scratch in a frame. Do this first.
2. **C-1** (MEDIUM, telemetry) — relabel `overflow_attempt_count` (or add a true per-frame demand signal) so the #1284 cap-sizing loop isn't fed a frame-multiplied number.
3. **C-2** (LOW, test) — pin the overflow guard so the M29-class silent-truncation regression can't return un-noticed.
4. **DIM12-A-01** (LOW, latent) — make the `SkinSlot` vertex-count invariant load-bearing (symmetric with the BLAS guard); defensive, not currently reachable.
5. **D12B-2 / DIM12-DOC-1** (LOW/INFO, doc rot) — one-line predicate-name fix + the count-comment refresh, fold into the next touch.

## Coverage Record — invariants verified to HOLD (no finding)

Deep-traced and confirmed correct in current code (selected): VERTEX_STRIDE_FLOATS=25 single-sourced from `shader_constants_data.rs` and pinned vs `size_of::<Vertex>()`; `skin_palette.comp` runs before `skin_vertices.comp` with a COMPUTE_WRITE→(COMPUTE|VERTEX)_READ barrier; 64-wide workgroup == `local_size_x` in both shaders, dispatch `div_ceil(64)`; `SkinPushConstants` (12 B) matches the GLSL block byte-for-byte; per-vertex bone-index clamp in both compute + raster paths; inline-skin (raster) vs pre-skin (RT) coexistence correct; output-buffer flags = `STORAGE | SHADER_DEVICE_ADDRESS | AS_BUILD_INPUT_READ_ONLY`; full COMPUTE→AS-BUILD→AS-BUILD(TLAS)→FRAGMENT barrier chain present with correct stage/access scopes; VUID-03667 enforced on both halves (`validate_refit_flags` + `validate_refit_counts`) with BLAS-drop on mismatch; `SKINNED_BLAS_FLAGS` shared by BUILD + UPDATE; skinned BLAS excluded from the M36 LRU sweep (pinned); scratch alignment asserted before every build; pose-dirty FNV-1a gate sound (first-sight always re-dispatches); bone-palette SSBO DEVICE_LOCAL + HOST_VISIBLE staging sized to `MAX_TOTAL_BONES`; over-cap bones return `None` (no OOB write) and cannot silently truncate a referenced slot; one-shot overflow warn latches correctly; descriptor pool sized to the bumped cap (not under-sized); `DebugStats` skin-pool telemetry wired end-to-end.

## Dedup notes (already known / fixed — not re-reported)

- **S12-1** (stale `BONE_PALETTE_OVERFLOW_WARNED` docstring at `resources.rs:646`, from the prior 2026-05-28 full report) — **fixed** by commit `69def52a`.
- **#1284** SkinSlotPool cap triple-bump + descriptor-pool pin + spill telemetry — landed; not a finding (its annotation drift is captured as DIM12-DOC-1).
