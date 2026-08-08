# Performance Audit — 2026-08-07

**Scope**: Full sweep, all 9 dimensions, depth=deep.
**Method**: Orchestrated — one Task agent per dimension, max 3 concurrent, each
doing a direct code read + live-test verification against the checklist in
`.claude/commands/audit-performance/SKILL.md`, cross-checked against
`gh issue list` (`/tmp/audit/performance/issues.json`, 47 open issues at
audit time) and the prior sweep (`docs/audits/AUDIT_PERFORMANCE_2026-08-03.md`,
4 days stale at audit time).

No live engine instance was launched this session (per the project's "No
Parallel Engine Launch" policy — the user was not confirmed to have no
instance of their own running). All findings are **code-verified**, not
freshly FPS-measured. Where a dimension's checklist called for a live
numeric capture (per-cell material dedup ratio, `ScratchTelemetry` numeric
values), that gap is called out explicitly in the dimension's own section
rather than papered over with an estimate.

## Executive Summary

**4 new findings this sweep: 0 CRITICAL, 2 HIGH, 2 MEDIUM, 0 LOW.**

| Dim | Area | New findings | Notes |
|---|---|---|---|
| 1 | CPU Per-Frame Allocations & Hot Paths | 0 | All 6 Session 46 guards intact |
| 2 | Draw-Call & Instancing Efficiency | 0 | All guards intact; 1 existing issue (#2351) re-confirmed open, not re-elevated |
| 3 | GPU Memory Pressure & Eviction Thrash | 1 MEDIUM | Fresh #863-class leak at a 2026-08-04 call site |
| 4 | SSBO Sizing & Per-Frame Upload | 0 | All guards intact, verified via live `cargo test`, not just code read |
| 5 | GPU Pipeline & Pass Efficiency | 0 | All guards intact; 2 existing issues (#779, #2367) re-confirmed open |
| 6 | Skinning & BLAS Cost (M29.x) | 1 HIGH | Regression of #1791/#1796 — rollback wired into only one of two `draw_frame` result arms |
| 7 | World Streaming & Cell Transitions (M40) | 0 | **All 3 prior findings (PERF-D7-01/02/03) confirmed FIXED since 2026-08-03**; #1798 still open (unchanged) |
| 8 | NIF Parse Performance | 1 HIGH + 1 MEDIUM | `allocate_vec<T>` allocation-amplification gap; 3 sites bypass the established bulk-read idiom |
| 9 | Telemetry & Camera-Relative Origin Cost | 0 | **PERF-D9-01 (prior LOW finding) confirmed FIXED** (`28155b79`, #2278) |

**Net trend since the 2026-08-03 sweep**: 4 of that report's 5 findings are
now closed (3 in Dimension 7, 1 in Dimension 9 — all fixed in the 4-day
window, most by same-day commits). One low-priority doc-rot item
(PERF-D-DOC-01, ROADMAP bench-of-record staleness) was independently
addressed by the 2026-08-04 R6a-stale-18 bench refresh (see ROADMAP.md,
unrelated to this sweep). Two of this sweep's four new findings are
regressions-in-the-broad-sense: not reversions of a landed fix's own code,
but the same bug *class* reappearing at a call site the original fix didn't
reach (a 2026-08-04 `NifImportRegistry::insert` call site for #863's
must-use contract; a `draw_frame` `Err` arm for #1791/#1796's
`skin_dispatch_ran` gate).

### Observed-vs-ROADMAP bench delta

No new bench run was performed this session (per the No Parallel Engine
Launch policy). ROADMAP.md's Bench-of-record block (`R6a-stale-18`, HEAD
`28155b79`, 2026-08-04) is 3 days old relative to this audit and — per its
own text — includes a same-session same-machine control rebuild that
separated real signal from machine-load/entity-count-growth noise. It is
not "stale" by this skill's threshold (ROADMAP itself only flags the block
once ~90+ commits or a substantial rendering/streaming change accumulates
since the refresh; 2026-08-04→2026-08-07 is a 3-day, low-commit-count
window with no rendering-critical-path change identified by Dimensions
1–6/9 in that span). No regression to report against it.

## Cross-Audit Coverage (what this report deliberately did not re-derive)

- **BLAS/TLAS budget formula, SSBO byte sizes, LRU thresholds, deferred-destroy
  countdown** — sourced from `docs/engine/memory-budget.md` throughout
  Dimensions 3–4, per this skill's standing instruction not to re-derive
  memory ceilings.
- **`GpuInstance`/`GpuMaterial` struct-layout regression tests** — re-run live
  (`cargo test -p byroredux-renderer gpu_instance` /
  `gpu_material_size_is_348_bytes`) rather than trusted from source comments,
  in Dimension 4.
- **BLAS eviction gap #1793 (permanently-missing rigid BLAS recovery,
  synchronous multi-cell false-eviction)** — both cross-linked from
  `/audit-renderer` Dim 1, unreachable on the 12 GB dev card, not
  re-investigated per the skill's explicit instruction.
- **`/audit-nifal` single-boundary contract** (PBR resolve-once, no per-draw
  `classify_pbr_keyword`) — verified as a downstream consumer in Dimension 4
  but the boundary's own correctness is that skill's territory, not
  re-litigated here.

## Findings

### PERF-D6-NEW-01: `draw_frame`'s `Err(e)` return arm never checks `skin_dispatch_ran` — the #1791/#1796 pose-hash/first-sight-upload corruption is still reachable through four early-`Err` paths the original fix didn't cover
- **Severity**: HIGH
- **Dimension**: Skinning & BLAS Cost (M29.x)
- **Location**: `byroredux/src/main.rs:784-897` (the `match ctx.draw_frame(...)` — rollback only lives in the `Ok(needs_recreate) =>` arm, lines 840-844; the `Err(e) =>` arm, lines 893-896, has no rollback) vs. `crates/renderer/src/vulkan/context/draw.rs:1347,1442,1458,1655` (four `return Err(e)` sites that execute strictly before `record_skinned_blas_refit` — the call that flips `skin_dispatch_ran` true at `draw.rs:2099` / `skinned_blas_refit.rs:63`)
- **Status**: Regression of #1791 / #1796 (the fix those issues shipped — the `skin_dispatch_ran` flag + the `if !ctx.skin_dispatch_ran { rollback... }` check in `main.rs` — was wired into only one of the two `match` arms `draw_frame`'s `Result` can produce)
- **Description**: #1791's own issue text explicitly named the loss vectors this fix needed to close: "empty framebuffers (`Ok(false)`)... `ERROR_OUT_OF_DATE_KHR`... **and fence/reset/begin error arms**." The landed fix (`skin_dispatch_ran`, reset `false` at the top of `draw_frame`, flipped `true` only once `record_skinned_blas_refit` runs) does cover the first two — but the consumer in `main.rs` only reads the flag inside the `Ok(needs_recreate) => { ... }` match arm. `draw_frame` can also return `Err(e)` — and does, from at least four sites that execute *before* `record_skinned_blas_refit` (i.e. while `skin_dispatch_ran` is still `false`): `wait_for_fences` on the image fence (`draw.rs:1339-1348`), `reset_command_buffer` (`draw.rs:1433-1444`), `begin_command_buffer` (`draw.rs:1449-1459`), and `build_fsr_frame_parameters` (`draw.rs:1637-1657`). On any of these, `main.rs`'s `Err(e) => { log::error!(...); event_loop.exit(); }` arm (`main.rs:893-896`) runs — and never calls `rollback_pending_pose_commits()` or `requeue_pending()`, regardless of what `ctx.skin_dispatch_ran` reads.
- **Evidence**:
  ```rust
  // byroredux/src/main.rs
  match ctx.draw_frame(FrameInputs { ..., pose_dirty: self.skin_slot_pool.pose_dirty() }) {
      Ok(needs_recreate) => {
          if !ctx.skin_dispatch_ran {                       // <- only checked here
              self.skin_slot_pool.rollback_pending_pose_commits();
              self.skin_slot_pool.requeue_pending(std::mem::take(&mut pending_for_requeue));
          }
          ...
      }
      Err(e) => {                                            // <- never checked here
          log::error!("Draw failed: {e:#}");
          event_loop.exit();
      }
  }
  ```
  and in `draw.rs`, all four sites below run before `self.skin_dispatch_ran = true;` at `skinned_blas_refit.rs:63` (reached via the call site at `draw.rs:2099`):
  ```rust
  // draw.rs:1339-1348 (image-fence wait)
  if let Err(e) = self.device.wait_for_fences(&[image_fence], true, u64::MAX)... { ...; return Err(e); }
  // draw.rs:1433-1444 (reset_command_buffer)
  if let Err(e) = self.device.reset_command_buffer(cmd, ...)... { ...; return Err(e); }
  // draw.rs:1449-1459 (begin_command_buffer)
  if let Err(e) = self.device.begin_command_buffer(cmd, &begin_info)... { ...; return Err(e); }
  // draw.rs:1637-1657 (FSR frame params)
  Err(e) => { ...; return Err(e); }
  ```
  Before every one of these, `self.skin_slot_pool.drain_pending(...)` has already irrevocably removed the first-sight `(slot, entity)` pairs from `SkinSlotPool::pending_uploads` (`main.rs:699-701`) — exactly the precondition #1791 described as "irrevocably drained... before invoking `ctx.draw_frame`." And the CPU-side pose-hash commit (`try_mark_pose_dirty`, called from `build_skinned_palettes` before `ctx.draw_frame` is even invoked) has already advanced `last_pose_hash` — exactly the precondition #1796 described. The regression test added for #1796 (`skin_dispatch_ran_ordering_tests` at `draw.rs:3721-3762`) only pins the *order* of the reset vs. the two `Ok`-path guards inside `draw_frame` itself — it says nothing about whether `main.rs`'s caller-side consumption of the flag covers the `Err` arm, so it doesn't (and can't) catch this gap.
- **Impact**: On any of these four (rare but real: driver/allocator pressure, not necessarily a fatal device-loss) failures, `draw_frame` returns `Err`. `event_loop.exit()` is *queued*, not synchronous — the same reasoning the codebase already relies on for the `framebuffers.is_empty()` guard means one or more further frames can still render before the process actually terminates. On such a frame: (1) drained first-sight `bind_inverses` for any entity that allocated a slot this frame are permanently lost, corrupting that entity's raster + RT skinning for its remaining lifetime in the cell (identical blast radius to #1791, HIGH); (2) any entity whose pose changed this frame has its `last_pose_hash` baseline advanced against a dispatch that never happened, freezing GPU output/BLAS one-plus frames stale if the pose then goes idle (identical to #1796, MEDIUM). Severity taken at the higher of the two component bugs.
- **Related**: #1791 (CLOSED, same root cause), #1796 (CLOSED, same root cause), #1194/#1195/#1196/#1197 (adjacent, unaffected guards, verified intact this sweep).
- **Suggested Fix**: Move the `if !ctx.skin_dispatch_ran { rollback... }` block out of the `Ok(needs_recreate) =>` arm so it runs after `ctx.draw_frame(...)` regardless of which arm matched — restructure to call `draw_frame`, capture the `Result`, do the `skin_dispatch_ran` check unconditionally, *then* match on the result for the rest of the per-arm handling. `ctx` is still valid and owned by `self` after an `Err` return.

### PERF-D8-NEW-01: `allocate_vec::<T>`'s remaining-bytes floor ignores `size_of::<T>()`, letting corrupt counts amplify into multi-gigabyte pre-read allocations
- **Severity**: HIGH
- **Dimension**: NIF Parse Performance
- **Location**: `crates/nif/src/stream.rs:258-271` (`allocate_vec`); worst-case call site `crates/nif/src/blocks/node.rs:1081` (`Vec<[f32; 16]>` in `BsDistantObjectInstancedNode::parse`); ~20 additional call sites across `crates/nif/src/blocks/{skin,interpolator,collision/*,tri_shape/*,controller/*}.rs` share the same bound function
- **Status**: NEW
- **Description**: `allocate_vec<T>(count: u32)` bounds `count` against `remaining` — the bytes left in the stream — treating every element as if it costs a minimum of **1 byte**:
  ```rust
  pub fn allocate_vec<T>(&self, count: u32) -> io::Result<Vec<T>> {
      let remaining = total.saturating_sub(pos);
      if (count as usize) > remaining { return Err(...); }
      Ok(Vec::with_capacity(count as usize))   // <-- count elements of size_of::<T>(), not count bytes
  }
  ```
  For any `T` whose real size is `size_of::<T>() = k > 1` bytes, a corrupt `count` up to `remaining` (which the check allows) requests a `Vec::with_capacity` of `count * k` bytes — up to **k× the actual data available**. This is exactly the failure mode `MAX_SINGLE_ALLOC_BYTES` / `check_alloc` were built to close for `read_bytes` and `read_pod_vec` (#113, #388, #764) — but `allocate_vec` never calls `check_alloc` and has no `size_of`-aware term at all. Compare with its sibling `read_pod_vec` (`stream.rs:355-365`), which computes `byte_count = count * size_of::<T>()` and passes that through `check_alloc` (both the remaining-bytes check **and** the 256 MB hard cap). `allocate_vec` is missing both.
  Worst concrete instance: `BsDistantObjectInstancedNode::parse` (Starfield distant-object-instancing node, live in the block dispatch table — `blocks/mod.rs:338-339`) does `stream.allocate_vec::<[f32; 16]>(num_transforms)?` — `size_of::<[f32; 16]>() = 64` bytes. NIF files legitimately range up to the archive-level `MAX_CHUNK_BYTES` cap of 1 GB (`crates/bsa/src/safety.rs:29-36`, vanilla content tops out around 325 MB on FO76's largest BA2). A corrupt/hostile file with e.g. 300 MB remaining and a forged `num_transforms` of 300,000,000 passes the `count > remaining` check (300M ≤ 300M) but requests `Vec::with_capacity::<[f32;16]>(300_000_000)` — **19.2 GB**. Any `allocate_vec::<T>` call with `size_of::<T>() > 1` (most of them — `NiTransform`-sized bone/ragdoll structs, `QuatKey`, `InterpBlendItem`, `BsGeometrySegmentData`, `(u64,u32)` tuples, etc.) is proportionally amplified by its own `size_of::<T>()`.
- **Evidence**: `crates/nif/src/stream.rs:258-271`; call site `crates/nif/src/blocks/node.rs:1057-1081`; dispatch confirming this parser is live: `crates/nif/src/blocks/mod.rs:338-339`; sibling function that *does* apply the size-aware cap: `crates/nif/src/stream.rs:355-365`; archive-level file-size ceiling: `crates/bsa/src/safety.rs:27-36`
- **Impact**: `Vec::with_capacity` failing an allocation calls Rust's default `handle_alloc_error`, which **aborts the process** — not a recoverable `io::Result::Err`. Even where the host has enough virtual memory, the multi-GB transient reservation is itself a DoS vector (thrash / OS OOM-killer), far outside the crate's own documented "256 MB is well above any legitimate single-block payload" invariant. Because `allocate_vec` is the shared primitive for ~20 non-bulk-array block parsers, the blast radius is every block type that uses it for a non-1-byte-element type, not just `BsDistantObjectInstancedNode`. A prior audit (`AUDIT_INCREMENTAL_2026-07-05.md`, disproof log entry for #1885) examined this bound but only from the false-positive-rejection angle; that reasoning is correct for legitimate files but does not cover the amplification direction addressed here.
- **Related**: #113, #388, #764, #831 (the `allocate_vec`/`check_alloc` hardening lineage this gap sits inside); #833/#1439 (`read_pod_vec`'s sibling, correctly size-aware); disproof log entry for #1885 in `docs/audits/AUDIT_INCREMENTAL_2026-07-05.md` (addresses a different question, doesn't cover this one)
- **Suggested Fix**: Give `allocate_vec` the same `check_alloc`-style guard `read_pod_vec` already has, scoped to `T: Sized` with a `checked_mul(count, size_of::<T>())`: reject if the byte product exceeds `remaining` *or* `MAX_SINGLE_ALLOC_BYTES`. This covers essentially all current non-POD-bulk `allocate_vec::<T>` call sites. Leave the existing loose 1-byte-per-element bound only for the small set of heap-indirect element types (`String`, `Vec<T>`, `Option<Arc<str>>`) where on-disk size can legitimately be smaller than `size_of::<T>()` — e.g. via a second helper (`allocate_vec_sized::<T>(count, min_wire_bytes_per_elem)`), so the fix doesn't reintroduce the false-positive risk the 2026-07-05 disproof log correctly flagged for those types.

### PERF-D3-NEW-01: `NifImportRegistry` LRU eviction drops freed `AnimationClipRegistry` handles in the precombined-mesh insert path
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure & Eviction Thrash
- **Location**: `byroredux/src/cell_loader/precombined.rs:313-316`
- **Status**: NEW (fresh reintroduction of the #863 bug class at a call site added 2026-08-04, `9e5540899` — not a regression of the original fix, whose three original call sites remain correct)
- **Description**: `NifImportRegistry::insert` returns `Vec<u32>` — the `AnimationClipRegistry` handles of any entries the 2048-cap LRU sweep evicted as a side effect of this insert — and is marked `#[must_use = "evicted clip handles must be released into AnimationClipRegistry to free their keyframe arrays — see #863"]`. Four of the five production call sites forward the returned handles to `AnimationClipRegistry::release` (`streaming_helpers.rs:260`, `partial.rs:69`, `partial.rs:173`, `references/mod.rs:686`). The precombined-mesh commit path does not:
  ```rust
  // byroredux/src/cell_loader/precombined.rs:313-316
  {
      let mut reg = world.resource_mut::<NifImportRegistry>();
      let _freed = reg.insert(path.clone(), parsed.clone());
  }
  ```
  Binding the `#[must_use]` return to a named variable (`_freed`, not the bare `_` discard) satisfies both the `must_use` and `unused_variables` lints, so the compiler gives no warning — the exact silent-drop shape #863's original bug had before the `Vec<u32>` contract was added.
- **Evidence**: `AnimationClipRegistry::release` (`crates/core/src/animation/registry.rs:156-176`) is what actually clears a slot's channel collections — skipping it leaves those collections (and their backing allocations) resident indefinitely. The precombine path's own inserted entry never itself owns a clip handle, but the LRU sweep triggered by *this* insert can evict any other cache entry once the registry is at cap (2048 default, or `BYRO_NIF_CACHE_MAX`), including animated NIFs registered via the three correctly-forwarding call sites — whichever victim the sweep picks, if it owned a clip handle, that handle is silently dropped here instead of released.
- **Impact**: A slow CPU-RAM leak (not VRAM) — bounded by `AnimationClipRegistry`'s slot count growing without corresponding frees, gated on (a) FO4 precombined-mesh content being loaded (M49), (b) the `NifImportRegistry` LRU cache being at its cap, and (c) the evicted victim happening to be an animated NIF with a registered clip handle. In a long FO4 session that revisits precombine-heavy cells repeatedly, this compounds the same way #863 originally did, just through a narrower door. Does not affect GPU VRAM directly, so it sits below the "resource leak that compounds per frame" HIGH floor (this compounds per LRU-eviction event, not per frame) — MEDIUM matches the severity #863 was filed at for the same bug class.
- **Related**: #863 (original fix, three-of-four-then-correct call sites), #544 (clip_handles map cleanup on eviction).
- **Suggested Fix**: Mirror `partial.rs:69`'s pattern — capture the returned `Vec<u32>` as `freed` (not `_freed`), and after the block, if non-empty, forward each handle to `world.resource_mut::<AnimationClipRegistry>().release(h)`.

### PERF-D8-NEW-02: Three per-element decode loops bypass the crate's own established bulk-read-then-map idiom for half-float/quaternion arrays
- **Severity**: MEDIUM
- **Dimension**: NIF Parse Performance
- **Location**: `crates/nif/src/blocks/extra_data.rs:377-385` (`BsPositionData::parse`), `crates/nif/src/blocks/node.rs:1080-1088` (`BsDistantObjectInstancedNode::parse`, transforms), `crates/nif/src/blocks/legacy_particle.rs:624-638` (`NiLegacyParticlesData::parse`, rotations)
- **Status**: NEW
- **Description**: #1263 (NIF-D5-NEW-03) and #2032 (PERF-D8-01) both established the same fix shape for "array needs a per-element transform the raw bytes don't carry" (half-float decode, byte-swizzle, etc.): bulk-read the raw fixed-width values in one `read_*_array` call, then `.chunks_exact(k).map(transform).collect()` — see `crates/nif/src/blocks/bs_geometry.rs:421-446` for the pattern now used for vertices/uvs0/uvs1. Three call sites never got the memo and still do `allocate_vec` + a per-element loop of individual `read_u16_le()`/`read_f32_le()` calls: `BsPositionData::parse` (per-vertex half-float blend-factor array, FO4/FO76 cloth/dismemberment), `BsDistantObjectInstancedNode::parse` transforms (`Vec<[f32; 16]>`, 16 individual `read_f32_le()` calls per transform instead of one bulk read + `chunks_exact(16)`), and `NiLegacyParticlesData::parse` rotations (reads `w,x,y,z` and reorders to `[x,y,z,w]` per quaternion, could bulk-read + swizzle in the `.map()`). None of these are per-frame (all are one-time import-side parses, cached after first load), so the impact is bounded CPU overhead on cell-load / streaming-worker latency, not steady-state frame time.
- **Evidence**: established idiom: `crates/nif/src/blocks/bs_geometry.rs:410-446`; three non-conforming sites at the locations listed above
- **Impact**: Extra per-element call overhead on the NIF-parse critical path for cell load / exterior streaming (a budget-bound path). Scales with vertex/instance count on FO4/FO76 cloth meshes and Starfield distant-object-instancing nodes; bounded by real-world content sizes, so this is a throughput/latency inefficiency rather than a correctness or memory-safety issue. dhat allocation-bound tests can't catch this class (allocation *count* is identical either way — the difference is N extra function-call/bounds-check/cursor-advance round trips instead of one bulk `read_exact`).
- **Related**: #1263 (NIF-D5-NEW-03, the original 3-site fix in `bs_geometry.rs`), #2032 (PERF-D8-01, the `BoneWeight` sibling fix in this exact dimension); the `node.rs` transforms site is also cited under PERF-D8-NEW-01 above for its separate allocation-bound issue — fixing the bulk-read shape here does not by itself fix that finding, both changes are complementary.
- **Suggested Fix**: Apply the same `read_*_array(count * k)?.chunks_exact(k).map(transform).collect()` shape at all three sites, mirroring `bs_geometry.rs:421-446`. For `node.rs`'s `[f32;16]` case, use `read_f32_array(count * 16)?.chunks_exact(16).map(|c| c.try_into().unwrap())`. Since dhat bounds can't catch this class, propose a wall-clock or read-call-count regression test alongside the fix.

## Existing / Re-Confirmed (not counted as new findings)

- **#2351** (Dim 2) — `bench_draws_batches` regressed on skyrim_se (3→8), same
  symptom class as the now-closed #2215 (closed today, `8dd03c1d`, as the
  deliberate depth-primary alpha-over sort-order cost, not a bug). #2351
  covers a fourth corpus (WhiterunDragonsreach) #2215's bisection didn't
  confirm against — genuinely needs a live headless bench run to close, not
  re-verifiable from code alone. Left open, not re-elevated.
- **#779** (Dim 5, `PERF-N6`) — `triangle.frag` still has no
  `layout(early_fragment_tests) in;`. Re-confirmed unaddressed. Added
  technical note this sweep: the shader's alpha-cutout `discard` sites
  (foliage/fence/signage) make the literal one-line fix a correctness
  trade (phantom depth writes surviving a discard), not a free win — needs
  RenderDoc validation before landing, per the skill's Speculative-Vulkan
  caveat.
- **#2367** (Dim 5, bench regression `3a02b02d..28155b79`) — still pending
  bisection. This sweep's read of the bounded path-traced GI code confirms
  it's architecturally sound and correctly capped (no O(mesh) scaling); FO4's
  denser interior geometry is a plausible, code-grounded contributor to the
  regression, supporting evidence for whoever picks up the bisection.
- **#1798** (Dim 7) — interior cell load still has no per-frame NPC-spawn
  budget (`load_cell_with_masters` → `load_references` →
  `FrameTimeBudget::unlimited()`); the exterior path has one
  (`STREAMING_APPLY_BUDGET`/`load_references_budgeted`), the interior call
  site was not migrated. Cost is now *measured* (`RefLoadAccum::npc_spawn_wall`,
  logged per-cell) rather than estimated, per the skill's #1798 note.
  Unchanged since the prior report.
- **#1793** (Dim 3, cross-linked from `/audit-renderer`) — permanently-missing
  rigid BLAS recovery + synchronous multi-cell false-eviction, both
  documented-not-fixed and unreachable on the 12 GB dev card. Not
  re-investigated per the skill's explicit instruction.
- **#1797** (Dim 6) — shared `blas_scratch_buffer` serialize ceiling across
  all skinned BLAS builds in a frame. Documented-not-fixed, pending
  measurement via `skin.coverage`/`gpu_skin_blas_refit_ms`. Not re-reported
  per the skill's instruction.

## Regressions Fixed Since the 2026-08-03 Sweep (verified, not re-reported)

| ID | Was | Fix commit | Verified this sweep |
|---|---|---|---|
| PERF-D7-01 | HIGH — worldspace-persistent-cell load bypassed budgeted streaming | rearchitected into resumable `PersistentCellApplyJob`, drained under `STREAMING_APPLY_BUDGET` via `advance_streaming_apply` | `app_step.rs:155`, `streaming_helpers.rs:305-329` |
| PERF-D7-02 | MEDIUM — `tag_descendants_as_actor` re-walked whole subtree per part attach | #2276 — tags from `part_root`, not `placement_root` | `npc_spawn/resumable.rs:1126-1139` + regression test |
| PERF-D7-03 | MEDIUM — SCOL/PKIN child-placement expansion recomputed on every resumed yield | #2277 — `current_ref_synth` persisted on `ReferenceLoadJob` across yields | `cell_loader/references/mod.rs:60-70, 409-458` |
| PERF-D9-01 | LOW — `gpu_timers.rs` had no per-bracket "ran this frame" flag | #2278 (`28155b79`) — `*_active` companion fields on `GpuTimerSnapshot` | `gpu_timers.rs:184-317` + 3 regression tests |

Only PERF-D-DOC-01 (ROADMAP bench-of-record staleness, LOW) from the prior
report has no dedicated fix commit — it was superseded by the ordinary
2026-08-04 R6a-stale-18 bench refresh cadence, unrelated to this audit.

## Regression Guards Verified Intact (no erosion found)

All guards named in the skill's 9 dimension briefs were individually
re-verified this sweep (full detail in each dimension's own section,
preserved for reference at time of writing in the per-dimension scratch
files). Highlights of what was *actually run*, not just read:

- **Dimension 4**: `cargo test -p byroredux-renderer gpu_instance` (3 layout
  tests), `gpu_material_size_is_348_bytes`, `scene_buffer::` (57 tests incl.
  4 `*_hash_tests` modules), `vulkan::material::tests::` (30 tests) — all
  live-run and passing, not inferred from source comments.
- **Dimension 1**: `drain_dirty_into` confirmed as the *only* production
  consumer of the dirty-set drain (grepped repo-wide for `take_dirty`
  production call sites — none found, only test-internal).
- **Dimension 5**: shipped `triangle.frag.spv` binary-searched for
  `resLight`/`resWSel`/`NUM_RESERVOIRS` strings to confirm `ENABLE_LEGACY_WRS`
  dead-code elimination is real in the compiled artifact, not just
  `#ifdef`-hidden with debug symbols surviving.
- **Dimension 9**: `git log --oneline -- crates/renderer/src/vulkan/gpu_timers.rs`
  used to confirm the PERF-D9-01 fix commit exists on the current branch and
  post-dates the prior report's investigation window.

No guard erosion found in any of the 9 dimensions.

## Hot Path Analysis

No live GPU-timer or `ScratchTelemetry` capture was taken this session (no
engine instance was launched, consistent with the prior sweep's own
methodology note). Per-pass GPU cost and per-cell material-dedup ratio
remain reachable via `bench-stats` / `ctx.scratch` / `skin.coverage` but
were not captured this pass — this is a repeated, explicitly-flagged gap
across both this and the prior sweep, not a silent omission.

What *was* code-verified as correctly wired for future numeric capture:
- `GpuPerFrameTimers`/`GpuTimerSnapshot` per-bracket `_active` flags (fixed
  this window, #2278) now let a future capture distinguish "pass didn't run"
  from "pass ran in 0.0 ms."
- `RefLoadAccum::npc_spawn_wall` (Dim 7, #1798 context) surfaces per-cell NPC
  spawn cost in the end-of-cell summary log — the measurement #1798's fix
  will need already exists.
- `ScratchTelemetry` (Dim 9) confirmed to read live `len()`/`capacity()` per
  scratch buffer with no stale/duplicated accounting — sound plumbing for a
  future numeric capture.

## Prioritized Fix Order

1. **PERF-D6-NEW-01** (HIGH) — quick, surgical fix: hoist the
   `skin_dispatch_ran` rollback check out of the `Ok` match arm in
   `main.rs` so it runs unconditionally. Small diff, closes a real (if rare)
   permanent-corruption path.
2. **PERF-D8-NEW-01** (HIGH) — add the `checked_mul(count, size_of::<T>())`
   guard to `allocate_vec`, mirroring `read_pod_vec`'s existing
   `check_alloc` call. One shared-primitive fix covers ~20 call sites.
3. **PERF-D3-NEW-01** (MEDIUM) — one-line fix: capture and forward the
   `Vec<u32>` return from `NifImportRegistry::insert` in
   `precombined.rs:313-316`, mirroring the three already-correct call sites.
4. **PERF-D8-NEW-02** (MEDIUM) — apply the established bulk-read-then-map
   idiom at the three named sites; bounded import-time CPU win, no
   correctness risk. Lowest urgency of the four (cell-load latency, not a
   safety or leak issue).

None of this sweep's findings require an architectural change — all four
are localized, single-site (or few-site) fixes with a clear existing
pattern to mirror.

---
*Compiled from 9 dimension sub-agent reports
(`/tmp/audit/performance/dim_1.md` … `dim_9.md`), deduplicated against
`gh issue list --repo matiaszanolli/ByroRedux` (47 open issues at audit
time) and cross-checked against `docs/audits/AUDIT_PERFORMANCE_2026-08-03.md`.*
