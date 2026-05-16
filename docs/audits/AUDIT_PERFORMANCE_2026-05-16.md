---
date: 2026-05-16
audit: performance
focus: dimensions 1 (GPU Pipeline), 2 (GPU Memory), 3 (Draw Calls), 7 (TAA & GPU Skinning), 8 (Material/SSBO Upload)
depth: deep
defers: dimensions 4 (ECS Queries), 5 (NIF Parse), 6 (CPU Allocations — partial, the skinning-loop allocs land in Dim 7 where they live), 9 (World Streaming) — recently audited; no material change
trigger: post-`1775a7e6` (R6a-prospector-regress, skinned-BLAS flag split) bench-of-record refresh
---

# Performance Audit — 2026-05-16

Follow-up to the `2026-05-10` performance audit, scoped to the renderer-deep
preset (Dim 1 / 2 / 3 / 7 / 8). The trigger is commit `1775a7e6` which
split `UPDATABLE_AS_FLAGS` into two per-acceleration-type constants
after a 191-commit bisect window — skinned BLAS now uses
`PREFER_FAST_BUILD | ALLOW_UPDATE` while TLAS keeps
`PREFER_FAST_TRACE | ALLOW_UPDATE`. Bench-of-record was refreshed to:

| Bench | FPS | wall_ms | fence_ms | Notes |
|---|---|---|---|---|
| FNV Prospector | 124.6 | 8.03 | 6.17 | GPU-bound (fence 77% of wall) |
| Skyrim Whiterun | 218.4 | 4.58 | — | 0 skinned BLAS — flag flip not a factor |
| FO4 MedTek | 67.1 | 14.91 | 5.50 | **CPU-bound** (`brd_ms=8.07` > fence) |

Audit focus is the renderer / draw-frame / skinning paths most affected by
the flag flip plus the now-quantified MedTek CPU-bound finding the user
flagged for surfacing.

## Executive Summary

| Severity | Count | Headline |
|---|---|---|
| CRITICAL | 0 | |
| HIGH     | 1 | PERF-D3-NEW-01 — `build_render_data` is the explicit CPU bottleneck on FO4 MedTek (`brd_ms=8.07 > fence=5.50`) and has known hot-path allocs |
| MEDIUM   | 4 | skinning per-frame allocs; effect-mesh substring scan; bone-palette fixed-stride zero-pad; instance SSBO has no dirty-gate |
| LOW      | 3 | stale UPDATABLE_AS_FLAGS docs in blas_skinned.rs; missing_samples Vec always allocated; tlas instance Vec amortized but the per-iteration format!() string allocs aren't |
| INFO     | 3 | PERF-GP-01 (2026-05-10) verified addressed via `VOLUMETRIC_OUTPUT_CONSUMED=false`; bench-of-record matches commit message; flag split itself is sound (no Vulkan VUID-pInfos-03667 risk) |
| **Total** | **11** | |

**Headline (Dim 3 / 7)**: PERF-D3-NEW-01 — `build_render_data` consumes
8.07 ms / frame on FO4 MedTek (10 810 entities, 7359 draws), vs the 5.50 ms
GPU fence — frame is now CPU-bound. Three contributing hot-path patterns
in the function:

1. 17 separate `world.query::<T>()` lock acquisitions per frame, with
   `GlobalTransform` taken 3 times distinct (lines 341, 514, 1070, 1298).
2. 6 case-insensitive substring scans (`contains_ci`) over every
   drawable mesh's texture_path (lines 660-666) — ~44K scans × ~600 byte
   comparisons each = ~26M byte comparisons/frame at MedTek scale.
3. Two fresh allocations per skinned mesh in `draw_frame`'s skin chain
   (`HashSet<EntityId>::new()` + `Vec<dispatch>::new()` + a third
   `first_sight_builds: Vec::new()`) at `draw.rs:610-688`. Skin-path
   only — not on the MedTek CPU-bound path (no humanoid skeleton.nif on
   FO4) but a known performance liability the moment FO4 skeleton support
   lands.

**Headline (Dim 8)**: instance SSBO upload (`upload_instances`,
`scene_buffer/upload.rs:228-258`) has no content-hash dirty-gate
counterpart to the one `upload_materials` got from #878 (PERF-D8-N-04).
A 7359-draw MedTek frame copies ~530 KB of GpuInstance bytes from host
to BAR memory every frame even when 90%+ of entities are static and
unchanged. Adding the same `last_uploaded_instance_hash` mirror saves
~32 MB/s sustained PCIe traffic at 60 fps once the scene settles —
defensible since the audit reference cites the same approach paid off
on `materials`.

**Headline (Dim 1 / 7)**: the `1775a7e6` flag split is sound (verified
in `acceleration/constants.rs:78-98`), but `blas_skinned.rs` still
carries 3 stale `UPDATABLE_AS_FLAGS` references in doc-comments and
one in dead "Flags: shared" log line at `blas_skinned.rs:288`. These
are doc-rot today but actively misleading for any future audit reader
trying to trace why skinned BLAS uses different flags than TLAS — and
the comment at `blas_skinned.rs:92-95` explicitly claims
`PREFER_FAST_TRACE` is in use, which is now false.

## Hot Path Analysis (per frame, FO4 MedTek at 1280×720, post-`1775a7e6`)

| Stage | Wall-clock | Notes |
|---|---|---|
| `build_render_data` CPU | **8.07 ms** | 🔴 PERF-D3-NEW-01 — `brd_ms > fence_ms`; 17 query locks, 6× substring scan per draw, ~7359 `to_gpu_material` hashes |
| Fence wait + draw submission | 5.50 ms | GPU work — proportional to draws, not CPU |
| UI tick + draw_call diagnostics | <0.5 ms | Negligible at MedTek scale |
| Skinning compute + BLAS refit | 0 ms | No humanoid skeleton on FO4 — skinned path skipped entirely |
| TLAS build (REFIT) | <0.1 ms | Static scene, REFIT dominates per PERF-GP-06 (2026-05-10) |

Prospector + Whiterun stay GPU-bound (fence dominates), so the audit's
single material lever for those benches is GPU-side. MedTek is the
canonical CPU-bound bench going forward.

## Findings

### HIGH

#### PERF-D3-NEW-01 — `build_render_data` is the explicit CPU bottleneck on FO4 MedTek
**Dimension**: Draw Call Overhead (Dim 3) — root cause spans Dim 6 (CPU Allocations)
**Location**: `byroredux/src/render.rs:299-1605`
**Status**: NEW (related to existing #1115 god-function complaint, but that's a code-size finding — this is the perf-impact framing)
**Description**: Bench-of-record at `1775a7e6` measures `brd_ms=8.07 > fence_ms=5.50` on FO4 MedTek (10 810 entities, 7359 draws). The frame is no longer GPU-bound on this bench; further GPU-side optimisations don't move the needle until `build_render_data` is reduced. Three identified contributors:

1. **17 query lock acquisitions per frame**, with `GlobalTransform` taken
   3 distinct times (`render.rs:341, 514, 1070, 1298`):
   ```
   gt_q (341)   skin_q (342)   cam_q (451)   transform_q (452)
   tq (514)     mq (515)       tex_q (516)   alpha_q (517)
   two_sided_q (518)   vis_q (519)   mat_q (520)   anim_uv_q (530)
   render_layer_q (539)   nmap_q (540)   dmap_q (541)   extra_q (542)
   terrain_tile_q (543)   wb_q (544)   gtq+eq for particles (1070-71)
   light_gt_q (1298)   light_q (1299)   tq (1353)   wq+fq (1514-15)
   ```
   Each `World::query` takes an `RwLockReadGuard`. Today these don't
   contend (render runs outside the scheduler), but the `#501 / M40`
   scheduler-parallelisation block-comment at `render.rs:495-505`
   acknowledges this is a ~1.5-2 ms latent stall the moment the
   scheduler parallelises.
2. **6 substring scans per draw, every frame** (`render.rs:660-666`):
   ```rust
   if contains_ci(tp, "effects\\fx") || contains_ci(tp, "effects/fx")
      || contains_ci(tp, "fxsoftglow") || contains_ci(tp, "fxpartglow")
      || contains_ci(tp, "fxparttiny") || contains_ci(tp, "fxlightrays")
   ```
   `contains_ci` runs `bytes.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))`
   — O(path_len × needle_len) per call. At MedTek scale (7359 draws ×
   6 scans × ~600 byte-cmps) ≈ 26M byte comparisons / frame, all to
   answer a question that's invariant across frames (the texture path
   doesn't change after spawn).
3. **Per-draw material hashing** — every `DrawCommand` materialises
   `material_hash()` (50-field hash) plus `to_gpu_material()` on dedup
   miss. Prospector dedup-hit rate is ~97% (per #781 / PERF-N4 doc) —
   MedTek's rate isn't telemetered today but the hash *itself* runs on
   every draw before dedup.

**Evidence**: ROADMAP.md Tier 11 + commit `1775a7e6` log explicitly call this out: *"frame still CPU-bound on `build_render_data` — that one is genuine and persists across the fix"*. Bench numbers (124.6 / 218.4 / 67.1) all post-fix; FO4 is the outlier.

**Impact**: Caps the achievable FPS ceiling on dense FO4 cells. M52 (mesh shaders / GPU-driven rendering) is the ROADMAP'd structural fix, but quick wins below would buy 1–3 ms back without M52's surface area.

**Suggested Fix order** (cheapest first):
1. **Static FX-skip classification at spawn**: lift the 6-needle
   substring scan out of `build_render_data` into a one-time
   `IsFxMesh` marker component set at `cell_loader` time. The needles
   look at the texture path, which never changes after spawn — this
   is a Phase-1-fits-the-pattern lift, ~50 LOC of cell_loader + a 1-bit
   component check in render.rs. Estimated saving: ~0.5-1 ms on MedTek.
2. **Material hash precomputation**: cache `material_hash()` on the
   `Material` component (or on a sibling) at spawn / when the material
   is mutated. The hash inputs are all from `Material`, `AlphaBlend`,
   `RenderLayer`, etc. — only the per-draw model_matrix is fresh, but
   the hash doesn't include it. Estimated saving: 0.2-0.5 ms.
3. **Single query bundle**: replace the 17 separate `world.query::<T>()`
   calls with a single 17-component `query_n!()` macro (doesn't exist
   today; would need ECS work) OR a `RenderExtract` resource populated
   by a system that runs once per scheduler tick and is consumed
   lock-free here. The comment at `render.rs:495-505` already names this
   path — recommend filing as a P1 follow-up to M40 parallelisation.
4. **par_iter over draws**: the per-draw loop (`render.rs:546-1049`)
   is embarrassingly parallel — each iteration writes one `DrawCommand`
   and one `material_table.intern_by_hash` (the table itself would need
   a `&Mutex` or per-thread accumulation + merge). Saves CPU at the
   cost of contention; only worth doing once #1115's split lifts the
   inner loop into a reviewable function. Subordinate to (1)-(3).

**Related**: #1115 (god-function size, separate dimension), M52 (GPU-driven rendering, structural fix), `feedback_audit_findings.md` (RenderDoc/profiler measurement required before claiming concrete ms savings).

**Profiling-infrastructure gap**: dhat / alloc-counter regression coverage
NOT wired. The ms-savings estimates above are CPU back-of-envelope; the
real measurement is `cargo flamegraph -- --bench-frames 600 --bench-hold`
on MedTek to confirm the contributors split as estimated. Without that
flame, ranking is by code-shape inspection only.

### MEDIUM

#### PERF-D7-NEW-01 — Skinned path allocates 3 fresh containers per frame in `draw_frame`
**Dimension**: TAA & GPU Skinning (Dim 7) / CPU Allocations
**Location**: `crates/renderer/src/vulkan/context/draw.rs:610, 611, 682`
**Status**: NEW (no prior finding — the per-frame scratch cluster doc at `context/mod.rs:781-803` lists 4 amortised scratches; these 3 don't qualify)
**Description**: Three per-frame `::new()` calls inside the skinning hot
path:
```rust
let mut seen: HashSet<EntityId> = HashSet::new();           // line 610
let mut dispatches: Vec<(EntityId, …)> = Vec::new();        // line 611
let mut first_sight_builds: Vec<(EntityId, …)> = Vec::new();// line 682
```
The block runs every frame the scene has skinned draws. On Prospector
that's 34 skinned NPCs → the Vecs grow to ~34 entries, the HashSet to
34 keys. `Vec` requires log₂(34) ≈ 6 reallocs to reach 34 capacity;
`HashSet` requires ~3 reallocs to reach a load factor that fits.

**Evidence**: Three `Vec::new()` / `HashSet::new()` literals at the cited line numbers. The scratch-cluster pattern (`context/mod.rs:781-803`) explicitly documents the convention for amortising this kind of buffer; these three don't follow it. None of the 4 documented scratches (`gpu_instances_scratch`, `batches_scratch`, `indirect_draws_scratch`, `terrain_tile_scratch`) cover the skinned-path case.

**Impact**: ~9 reallocs × 34 skinned NPCs × 60 fps = ~18 K reallocs/s on
Prospector. Allocation-cost-wise this is in the ~100 µs range and
well below the FPS-signal threshold for the renderer-deep audit — but
it's the simplest "fits the existing convention" cleanup available
right now, and the skinned-mesh path is the one that just got refactored
(commit `1775a7e6`'s flag split).

**Suggested Fix**: Add three sibling scratches to the `*_scratch` cluster
in `context/mod.rs`:
```rust
skin_dispatch_seen_scratch: HashSet<EntityId>,
skin_dispatches_scratch: Vec<(EntityId, SkinPushConstants, vk::Buffer, u32, u32)>,
skin_first_sight_builds_scratch: Vec<(EntityId, vk::Buffer, u32, vk::Buffer, u32)>,
```
`mem::take` / `clear()` at the top of the block, `mem::replace` back at
the end — same pattern as `gpu_instances_scratch`. ~20 LOC. Estimated
saving below FPS-signal threshold today but free maintenance hygiene.

**Profiling-infrastructure gap**: alloc-counter regression NOT wired. The
saving is "estimated"; no test today would catch a re-regression.

#### PERF-D8-NEW-01 — Instance SSBO upload has no content-hash dirty-gate
**Dimension**: Material Table & SSBO Upload (Dim 8)
**Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:228-258`
**Status**: NEW (sibling to existing #878 / PERF-D8-N-04 which added the same gate to `upload_materials`)
**Description**: `upload_instances` unconditionally `copy_nonoverlapping`s
`count × sizeof(GpuInstance)` bytes from host to the BAR-mapped
`instance_buffers[frame_index]` every frame, regardless of whether
this frame's instance set is byte-identical to last frame's. The
sibling `upload_materials` got a content-hash dirty-gate in #878 with
the explicit doc comment *"Static interior cells produce a byte-identical
materials slice every frame; skipping the copy + flush in steady state
saves ~3 MB/s sustained PCIe traffic"*. The instance buffer is much
larger — at 72 B per `GpuInstance` × 7359 MedTek draws = 530 KB/frame
vs the material table's ~52 KB/frame at the same load.

**Evidence**: `upload.rs:228-258` shows no `last_uploaded_instance_hash` field on `SceneBuffers` (search at `buffers.rs:85` confirms only `last_uploaded_material_hash` exists). The buffer is always rewritten.

**Impact**: ~32 MB/s sustained PCIe traffic on MedTek at 60 fps when the
camera is parked in a static interior — fully wasted work since the
content is identical frame-to-frame. The bandwidth itself isn't the
ceiling (PCIe 4.0 × 16 = 31.5 GB/s), but the host-side copy + flush
costs ~50-100 µs of CPU time per frame. On a CPU-bound bench like
MedTek this is part of the `brd_ms` problem.

**Disprove attempt**: GpuInstance carries `model_matrix` which IS
typically different every frame (animated entities, camera-relative
content). True for skinned + particle paths. But MedTek has 0 skinned
NPCs (no humanoid skeleton.nif) and the static-mesh fraction is the
overwhelming majority of the 7359 draws — most entities are walls,
floors, furniture, decals. For those, model_matrix is identity per
spawn and never mutated. The hash would catch the all-static case
cleanly; even on dense scenes with some animation, the hash check
itself is ~80 µs (530 KB / SipHash ≈ 6 GB/s ≈ 80 µs at 530 KB).

**Suggested Fix**: Mirror the `last_uploaded_material_hash` field +
gate logic exactly:
```rust
// On SceneBuffers struct
last_uploaded_instance_hash: [Option<u64>; MAX_FRAMES_IN_FLIGHT],

// In upload_instances
let hash = hash_instance_slice(&instances[..count]);
if self.last_uploaded_instance_hash[frame_index] == Some(hash) {
    return Ok(());
}
// … existing copy + flush …
self.last_uploaded_instance_hash[frame_index] = Some(hash);
```
~30 LOC. The `hash_instance_slice` helper can reuse the SipHash-1-3
infrastructure that already lives in `scene_buffer/descriptors.rs:11`
(`hash_material_slice`).

**Related**: #878 / PERF-D8-N-04 (materials dirty-gate, the template). Light buffer (`upload_lights` at `upload.rs:19-56`) is also missing this gate but has 100× less volume; lower priority.

#### PERF-D7-NEW-02 — Fixed-stride bone palette wastes ~6.4 KB per skinned mesh on partial poses
**Dimension**: TAA & GPU Skinning (Dim 7) / GPU Memory
**Location**: `byroredux/src/render.rs:429-436`, `crates/core/src/ecs/components/skinned_mesh.rs:29` (`MAX_BONES_PER_MESH = 128`)
**Status**: NEW
**Description**: Every skinned mesh's bone palette is zero-padded to
`MAX_BONES_PER_MESH = 128` slots so per-mesh `bone_offset` arithmetic in
the shader is trivially `offset + local_index`. The padding loop at
`render.rs:429-436`:
```rust
for _ in palette_scratch.len()..MAX_BONES_PER_MESH {
    bone_palette.push([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                       [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]);
}
```
A typical FNV/Skyrim humanoid skeleton.nif has ~75-90 active bones;
the remaining 38-53 slots are identity-padded every frame. At 34
Prospector NPCs × ~50 slots × 64 B = ~109 KB/frame wasted upload + GPU
storage. Times 60 fps = ~6.5 MB/s sustained PCIe traffic.

**Evidence**: The padding loop unconditionally runs the bone-count gap
on every skinned mesh every frame. `MAX_BONES_PER_MESH = 128` is sized
for worst-case (Skyrim+ male skeleton with extra IK bones); no NIF in
the FNV/FO3/SE corpus actually fills it. The shader contract
(`triangle.vert` reads `bones[bone_offset + bone_idx]`) requires the
stride match — variable-stride would need a per-mesh bone_count uniform.

**Impact**: ~6.5 MB/s sustained PCIe + ~272 KB GPU storage waste
on Prospector. Below FPS-signal threshold today; matters more once
M41 scales NPC counts (multiple cells worth of NPCs simultaneously
during exterior streaming would push toward ~600 active skinned meshes,
6× the current 34).

**Suggested Fix** (M29.5 follow-up — already on the ROADMAP):
implement variable-stride packing — store `bone_offset` AND
`bone_count` per skinned mesh, shader reads
`bones[bone_offset + min(bone_idx, bone_count - 1)]`. Saves the
identity-padding loop and the bytes. Marked as **M29.5 GPU palette
dispatch** in CLAUDE.md — recommend treating this finding as the
budget reference for that milestone rather than a standalone fix.

**Disprove attempt**: variable-stride breaks the
`gpu_instance_does_not_re_expand_with_per_material_fields` test
contract — false, the test pins `GpuInstance` size only; bone_offset
+ bone_count would land on `SkinnedMesh` or a sibling, not on
`GpuInstance`.

#### PERF-D3-NEW-02 — Per-draw effect-mesh substring scan runs every frame on invariant data
**Dimension**: Draw Call Overhead (Dim 3)
**Location**: `byroredux/src/render.rs:651-668`
**Status**: NEW (sub-finding of PERF-D3-NEW-01 above — separable fix)
**Description**: This is the (2) contributor from PERF-D3-NEW-01,
broken out as a standalone finding because it's the cheapest standalone
fix in the bundle and the most testable for ms-impact regression.
```rust
fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.as_bytes()
        .windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}
if contains_ci(tp, "effects\\fx") || contains_ci(tp, "effects/fx")
   || contains_ci(tp, "fxsoftglow") || contains_ci(tp, "fxpartglow")
   || contains_ci(tp, "fxparttiny") || contains_ci(tp, "fxlightrays") { continue; }
```

**Evidence**: 6 substring scans × every drawable mesh × every frame. The texture path is invariant across frames after spawn — this is the textbook "lift invariant out of inner loop" candidate.

**Impact**: ~0.5-1 ms / frame at MedTek scale (rough estimate; no
profiling today). On the CPU-bound bench this could shift the
`brd_ms / fence_ms` ratio.

**Suggested Fix**: Add a marker `IsFxMesh` component (or extend
`RenderLayer` with an `Effect` variant — already has Architecture /
Clutter / Actor / Decal slots). Set at NIF-import time in
`cell_loader::spawn_nif_entity` when the texture path matches any of
the 6 needles. The render-data loop becomes:
```rust
if fx_q.as_ref().and_then(|q| q.get(entity)).is_some() { continue; }
```
~30 LOC across cell_loader + render.rs. The classification is one-time
at cell load — fits the M40 streaming budget model (work moved out of
the per-frame loop). Estimated saving 0.5-1 ms / frame on MedTek.

### LOW

#### PERF-D2-NEW-01 — Stale `UPDATABLE_AS_FLAGS` doc-comments in `blas_skinned.rs` post-`1775a7e6`
**Dimension**: GPU Memory & Allocation (Dim 2) — doc rot from this session
**Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:92-98, 288, 650`
**Status**: NEW (introduced by `1775a7e6` itself — the flag-name change touched the constant import but missed the surrounding doc-comments)
**Description**: Three doc-comments in `blas_skinned.rs` still reference
the old `UPDATABLE_AS_FLAGS` name and the `PREFER_FAST_TRACE` rationale
that the `1775a7e6` flag split explicitly contradicts:
- **Line 92-98** (`build_skinned_blas`): *"Build flags: see `UPDATABLE_AS_FLAGS` for the shared `PREFER_FAST_TRACE | ALLOW_UPDATE` rationale (#679 / REN-D8-NEW-08: skinned BLAS refits in-place ~600 frames between full builds, so trace cost dominates by ~6 orders of magnitude)."*  This was the buggy assumption that drove the regression; the post-fix code uses `SKINNED_BLAS_FLAGS = PREFER_FAST_BUILD | ALLOW_UPDATE`.
- **Line 288** (`build_skinned_blas_batched_on_cmd`): *"Flags: shared `UPDATABLE_AS_FLAGS` — see #958 / REN-D8-NEW-14."*  Same issue — the function now uses `SKINNED_BLAS_FLAGS`, not `UPDATABLE_AS_FLAGS`.
- **Line 650** (`refit_skinned_blas`): *"The shared `UPDATABLE_AS_FLAGS` constant guarantees this UPDATE's flag set matches the original BUILD (VUID-…-pInfos-03667)."*  The VUID-03667 invariant is still satisfied (BUILD + UPDATE both use `SKINNED_BLAS_FLAGS`), but the named constant is wrong.

**Evidence**: Confirmed via `grep -n "UPDATABLE_AS_FLAGS" crates/renderer/src/vulkan/acceleration/blas_skinned.rs` against post-`1775a7e6` HEAD. Constant import at line 12 is `SKINNED_BLAS_FLAGS`; the 4 call sites at 101, 165, 330, 655 all use the new name; only the 3 doc-comments drifted.

**Impact**: Doc rot, no runtime effect. Active hazard for the next reader who chases through the skinned-BLAS flag rationale — the doc-comment says `PREFER_FAST_TRACE` while the code reads `PREFER_FAST_BUILD`, which is exactly the bench-bisect mistake `1775a7e6` is *fixing*.

**Suggested Fix**: 3 doc-comment edits — replace `UPDATABLE_AS_FLAGS` with `SKINNED_BLAS_FLAGS` and update the rationale text to point to R6a-prospector-regress rather than #679. ~5 LOC, zero test impact. The constant's own doc-comment at `constants.rs:84-93` is correct and forms the canonical reference; the three sites just need to point there.

**Related**: `1775a7e6` (this session). This is the same class of finding as TD7-* — stale path/symbol references in audit reports — but inside production code-comments. Worth a `audit-incremental` pickup once that audit family reaches blas_skinned.

#### PERF-D8-NEW-02 — TLAS build allocates a `Vec<String>` of missing-BLAS samples every frame
**Dimension**: Material Table & SSBO Upload (Dim 8) — misnamed dim, lives in TLAS
**Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:87`
**Status**: NEW
**Description**: `let mut missing_samples: Vec<String> = Vec::new();`
allocates every frame regardless of whether any draw is actually
missing a BLAS. On healthy steady-state scenes the Vec is dropped
empty. The Vec itself is trivial; the per-iteration
`format!()` allocations inside the missing-BLAS branches at lines
118-120, 130-132, 144-149 could grow up to 5 entries — bounded but
heap-touching.

**Evidence**: Direct read of `tlas.rs:87`. The `instances` Vec one line earlier (line 66) IS amortised via `mem::take(&mut self.tlas_instances_scratch)` — good pattern; `missing_samples` doesn't follow it.

**Impact**: 1 empty Vec alloc/frame on healthy frames (~24 B). Below FPS-signal threshold; lift only if a broader scratch-cluster sweep picks up `tlas.rs` siblings.

**Suggested Fix**: Either follow the `tlas_instances_scratch` pattern (add `tlas_missing_samples_scratch` to `AccelerationManager` and `mem::take` it here), OR replace with a fixed-size `[Option<String>; 5]` array — the upper bound is `MISSING_BLAS_SAMPLE_LIMIT = 5`. The array would zero per-frame heap allocations except on the (rare) miss path. ~10 LOC.

#### PERF-D1-NEW-01 — Volumetric draw.rs dispatch gate uses runtime `if` on host const
**Dimension**: GPU Pipeline (Dim 1)
**Location**: `crates/renderer/src/vulkan/context/draw.rs:1410, 2191`
**Status**: NEW (minor — addressed-but-not-optimal follow-up to the
2026-05-10 audit's PERF-GP-01)
**Description**: `VOLUMETRIC_OUTPUT_CONSUMED` is a `const bool` set to
`false` (`volumetrics.rs:124`). Two call sites read it as a runtime
condition:
```rust
if super::super::volumetrics::VOLUMETRIC_OUTPUT_CONSUMED { ... }  // draw.rs:1410, 2191
```
The const-bool gate works (the compiler dead-code-eliminates the
inner branch when `false`), but the *enclosing if-let chain* doesn't
get the same DCE treatment if the compiler can't prove
`self.volumetrics.is_some()` is the only check protecting state
needed by the false branch. RenderDoc capture would confirm whether
the vol.dispatch call site DOES get folded out today.

**Evidence**: Both call sites cited; the const is reachable at compile
time. The 2026-05-10 audit's PERF-GP-01 was closed by adding the
const gate, but the fix is one optimisation pass away from being
"dispatches actually skipped at runtime."

**Impact**: Currently un-measured. If LLVM does fold this out the impact is zero; if not, ~10-20 ms/frame on cells with TLAS (the original PERF-GP-01 magnitude). The audit recommendation is to verify under RenderDoc / Nsight rather than re-fix preemptively.

**Suggested Fix**: Replace the runtime `if` with `#[cfg(feature = "volumetrics")]` (Cargo feature, flipped to `true` when M-LIGHT v2 lands) so the dispatch site disappears from the binary entirely on `false`. Alternative: lift the const to a `pub const fn` and gate at compile time via `const { ... }`. ~5 LOC.

### INFO

#### PERF-D1-INFO-01 — Volumetric dispatch IS gated, per the 2026-05-10 audit's PERF-GP-01 fix
**Files**: `crates/renderer/src/vulkan/volumetrics.rs:124`, `context/draw.rs:1410, 2191`. The 2026-05-10 audit's largest-impact finding has been addressed via the `VOLUMETRIC_OUTPUT_CONSUMED = false` host-side const + matching shader gate. Confirmation, not a new finding.

#### PERF-D7-INFO-01 — Skinned-BLAS flag split is sound and tests pass
**Files**: `crates/renderer/src/vulkan/acceleration/constants.rs:62-98`. The `SKINNED_BLAS_FLAGS` / `UPDATABLE_AS_FLAGS` split satisfies VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667 (BUILD and UPDATE call sites use the same flag set per-acceleration-type). 242 renderer tests pass post-fix per commit log. Confirmation, not a new finding.

#### PERF-D3-INFO-01 — Bench-of-record matches commit message
The bench numbers (124.6 / 218.4 / 67.1) in commit `1775a7e6`'s log match what `ROADMAP.md` Tier 11 / Status banner reports as of HEAD. No drift to flag; the audit's references are pinned to the same numbers the user provided.

## Prioritized Fix Order

1. **PERF-D2-NEW-01** (LOW, code hygiene) — fix the 3 stale `UPDATABLE_AS_FLAGS` doc-comments in `blas_skinned.rs`. ~5 LOC. Zero risk, immediate value to the next audit reader. Recommend folding into the same session that landed `1775a7e6`.
2. **PERF-D3-NEW-02** (MEDIUM, MedTek win) — lift the 6-needle effect-mesh substring scan into an `IsFxMesh` marker component at cell_loader spawn time. ~30 LOC. Estimated saving 0.5-1 ms / frame on FO4 MedTek (the CPU-bound bench).
3. **PERF-D8-NEW-01** (MEDIUM, all-bench win) — mirror the `last_uploaded_material_hash` dirty-gate pattern onto `upload_instances`. ~30 LOC. Estimated saving 50-100 µs CPU / frame + 32 MB/s PCIe on static scenes.
4. **PERF-D7-NEW-01** (MEDIUM, code hygiene) — add 3 sibling scratches to the per-frame cluster in `context/mod.rs` so the skinning path matches the existing convention. ~20 LOC. Below FPS-signal threshold today; pre-empts a future scale finding.
5. **PERF-D3-NEW-01** (HIGH, architectural) — the umbrella finding. (2) is covered by item 2 above. (3) needs M40-parallel-scheduler work to fully unlock. Recommend filing the umbrella as P1 referencing items 2-4 + a follow-up issue for the material-hash precomputation contributor.
6. **PERF-D7-NEW-02** (MEDIUM, M29.5 follow-up) — variable-stride bone palette packing. Already on the ROADMAP; this finding is the budget-impact lens.
7. **PERF-D8-NEW-02** (LOW, code hygiene) — TLAS `missing_samples` Vec scratch lift. ~10 LOC.
8. **PERF-D1-NEW-01** (LOW, verification rather than fix) — confirm volumetric dispatch gate is dead-code-eliminated under RenderDoc / Nsight; if not, switch to `#[cfg(feature)]` gate.

## Dimensions Deferred

| Dim | Last audited | Reason for deferral |
|---|---|---|
| 4 — ECS Query Patterns | 2026-05-04 | Baseline locked (#823–#828); render.rs query count flagged in PERF-D3-NEW-01 instead |
| 5 — NIF Parse | 2026-05-04 | NIF parser stable; no parser-touching commits this session |
| 6 — CPU Allocations | merged into Dim 7 above | The skinning-loop allocations land naturally in Dim 7 |
| 9 — World Streaming | 2026-05-06b | CELL-PERF-01/02/03 still open; separate work track |

## Cross-References

- Prior canonical: `docs/audits/AUDIT_PERFORMANCE_2026-05-10.md` (closes PERF-GP-01 via volumetric gate)
- Triggering commit: `1775a7e6` (R6a-prospector-regress, skinned-BLAS flag split)
- Bench-of-record source: ROADMAP.md Status banner + Tier 11 narrative
- Related infrastructure gaps:
  - dhat / alloc-counter regression coverage NOT wired (recurring; flagged in 2026-05-04 + 2026-05-06 + 2026-05-10 audits)
  - RenderDoc / Nsight profiling NOT run on this audit's findings; ms-savings are CPU back-of-envelope from code-shape inspection only
- Related issues:
  - #1115 (build_render_data god-function size — code-quality framing of PERF-D3-NEW-01's substrate)
  - #877 (NIF-PERF-13 pre_parse_cell BSA mutex — orthogonal, not in scope)
  - #878 (PERF-D8-N-04 material dirty-gate — the template PERF-D8-NEW-01 copies)
  - M29.5 / GPU palette dispatch (PERF-D7-NEW-02's structural home)
  - M52 / GPU-driven rendering (PERF-D3-NEW-01's structural fix)

---

*Audit run by orchestrator (Skill tool denied; fell back to manual protocol per `.claude/commands/audit-performance.md`). Total: 11 findings across 5 dimensions; 4 dimensions deferred to recent canonical audits.*
