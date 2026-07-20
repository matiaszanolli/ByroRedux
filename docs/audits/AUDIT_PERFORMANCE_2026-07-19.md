# Performance Audit — ByroRedux — 2026-07-19

**Scope:** all 9 dimensions, depth = deep.
**Hardware target:** RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t). RT VRAM min 6 GB.
**Bench-of-record:** ROADMAP HEAD `8a668eff` (2026-07-18, R6a-stale-15 refresh).
**Dedup baseline:** `gh issue list` returned empty in this session (no network/auth);
sub-agents that had cached access checked against 200 issues. Findings that name an
existing issue are marked *Existing*; unverified NEW findings should be de-duped against
live issues before filing.

---

## Executive Summary

This is overwhelmingly a **guard-confirmation** audit. Every Session-46 (#1371–#1379),
Session-47 (#1489–#1498), and the intervening perf/skinning regression guards
(#1194–#1197, #1430, #1791–#1804, #1794, #1799, #1809, #2036) were traced to live code
and are **intact — zero erosion**. No CRITICAL or HIGH findings. No new hot-path
allocation or O(capacity) upload was found; per-frame render/ECS/SSBO paths are clean.

**Findings by severity:**

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 3 | DIM2-01 (opaque RT overdraw, no depth pre-pass), D7-02 (interior sub-cell spawn budget — *known*), D8-01 (streaming header re-parse for `bsver`) |
| LOW      | 8 | D4-01*, D5-01, D5-02, D6-01, D7-01, D8-02, D8-03, D9-01 |

(* D4-01 is a documented residual of already-closed #1794.)

### Observed-vs-ROADMAP bench delta

Benches were **not re-run in this session** (headless Vulkan + on-disk game data is
smoke-test territory; the debug build is too slow to stream the dense fault cells).
Against the ROADMAP `8a668eff` refresh, the standing deltas vs the prior record
(`1c26bc25`) are: **Prospector (FNV) +90.4% FPS / fence −54.5%**, **FO4 MedTek +14.1%
FPS / fence −21.6% at +47% entities**, **Whiterun (Skyrim SE) −7.7% FPS / fence +23.5%**.
The Skyrim dip sits inside its own 3-run range (313.9–364.4 FPS straddles the prior
362.8), so it reads as run-to-run noise rather than a structural regression — no finding
in this audit accounts for or contradicts it. None of the code findings below are
expected to move these numbers materially; DIM2-01 is the only one with fragment-bound
FPS upside, and it is gated behind a RenderDoc measurement.

---

## Hot Path Analysis

**Per-frame CPU (Dim 1, 9):** No fresh `Vec`/`HashMap`/`String` in steady state on any
traced system. `drain_dirty_into` capacity-preservation, `AnimScratch`/`last_cam`
scratch reuse, `bone_world` no-clear reuse, `emit_particles` dead-probe removal — all
hold. The one residual per-frame allocation is a diagnostic-path `format!` (D9-01, LOW).
*No dhat coverage exists for render/ECS hot paths (profiler is a process singleton) —
these confirmations rest on code reading + named unit guards, not allocation traces.*

**Per-frame SSBO upload (Dim 4):** Every upload (`upload_instances` / `_materials` /
`_lights` / `_indirect` / `_bone_worlds`) is O(live count), content-hash dirty-gated
(#1134/#878/#1809/#2036), and flushes only the written prefix. Material dedup is O(1)
amortized, upload size O(unique materials). `GpuInstance` = 112 B per-draw-only; PBR
resolved once at import (plain `f32`, no per-draw classify). No O(capacity) memcpy exists.

**Per-pass GPU (Dim 5):** Volumetrics O(froxels) (fixed 160×90×128), bloom O(pixels),
6-attachment G-buffer, `inv_vp` CPU-side (no per-fragment `inverse()`), Disney lobe gated
on `contribution < 0.001`, TLAS build→read barrier present, legacy WRS compiled out
(`ENABLE_LEGACY_WRS=0`). GPU per-pass timers (`gpu_timers.rs`) read back non-blocking
from the ~2-frame-old fence-retired pool — no current-frame stall (Dim 9 KEY guard).

**Skinning (Dim 6):** Dispatch-dirty gate (`pose_dirty`), BLAS refit gate,
descriptor-rewrite skip, early-return rollback all intact; `SKINNED_BLAS_FLAGS` stays
`FAST_BUILD`. Shared-scratch serialization (#1797) remains a documented, measured-first
ceiling — not re-reported.

**Streaming (Dim 7):** Two-phase `pre_parse_cell` (#877) + small-model serial fast-path
(#1262) intact; NIF import cache is process-lifetime; CDB header-probed once per archive.

**NIF parse (Dim 8):** `read_pod_vec` bulk reads, `#[must_use] allocate_vec`, split
per-block counters, import-only particle walk all intact.

---

## Findings (grouped by severity, CRITICAL first)

### MEDIUM

#### DIM2-01 — Opaque pass runs the RT fragment shader on occluded fragments (no depth pre-pass)
- **Dimension:** Draw & Instancing · **Status:** NEW
- **Location:** `crates/renderer/src/vulkan/context/helpers.rs:160-163` (single subpass),
  `byroredux/src/render/mod.rs:450-459` + `:257-270` (opaque sort: `mesh_handle` primary,
  `sort_depth` only a within-mesh tiebreak), `crates/renderer/src/vulkan/context/draw.rs:1176`
  (opaque batches bind `triangle.frag` = PBR + RT ray queries).
- The main geometry pass is a single subpass with no depth-only pre-pass, and the sort
  key orders by `mesh_handle` first (for instancing) so across meshes there is no
  front-to-back ordering. Occluded opaque fragments run the full PBR+RT ray-query shader
  and are only then rejected by the depth test. `discard`/alpha-test meshes disable HW
  early-Z regardless of order. The in-code comment acknowledges the batching-vs-early-Z
  trade but understates the cost side: it's RT ray-query work, not just ALU.
- **Impact:** Fragment-bound dense interiors/exteriors pay RT shader cost for invisible
  fragments, scaling with overdraw depth × RT ray count — worst on the RT-mandatory
  target GPU.
- **Fix:** Add a cheap depth-only pre-pass (position-only VS, no/minimal FS) reusing the
  existing instanced batches — restores full early-Z while keeping mesh-primary
  instancing. **Gate behind a RenderDoc measurement on a real cell before committing**
  (per the speculative-Vulkan posture; the win is real only if these frames are
  fragment-bound).

#### D7-02 — Interior / sub-cell spawn budget absent (known; now partially measured)
- **Dimension:** Streaming & Cells · **Status:** Existing (prior AUDIT_PERFORMANCE_2026-07-02; timing added by #1798)
- **Location:** `byroredux/src/cell_loader/references/mod.rs:755-860` (synchronous per-REFR
  spawn), `byroredux/src/app_step.rs:19` (`MAX_CELLS_SPAWNED_PER_FRAME` gates whole exterior
  cells only, not this path).
- `load_references` spawns every static + NPC for an interior cell in one synchronous
  pass, no sub-cell cap or yield. A large interior pays its entire parse + spawn + BLAS +
  batched texture flush/fence in one frame. #1798 added `npc_spawn_wall` timing to the
  end-of-cell summary so the NPC share is now measured, but no throttle exists. The
  LOD-ring spawn (`terrain_lod.rs:239`) shares the root cause on teleport.
- **Fix (deferred / M40 follow-up):** amortise `load_references` across frames with a
  sub-cell spawn budget; extend the per-frame cap to the LOD-ring burst. Cite
  `npc_spawn_wall` for the NPC share rather than estimating.

#### D8-01 — Streaming worker re-parses the whole NIF header just to read `bsver`
- **Dimension:** NIF Parse · **Status:** NEW · **Quick win**
- **Location:** `byroredux/src/streaming.rs:517-522`.
- `parse_one_nif` calls `NifHeader::parse(&bytes)` a **second** time (after `parse_nif`
  already succeeded) only to read `user_version_2` — but the just-parsed `NifScene`
  already retains it as `scene.bsver` (`scene.rs:124`, set at `lib.rs:829`). The re-parse
  re-walks and re-allocates the entire header string + block-type tables (one `Arc<str>`
  per string), not the "~60 bytes" the inline comment claims. Runs for every NIF on the
  background cell-load worker; Skyrim SE/FO4/FO76/Starfield meshes carry hundreds of
  strings, doubling header allocation count per NIF.
- **Fix:** replace with `let bsver = scene.bsver;` (binding already in scope at `:509`)
  and correct the comment. Zero extra allocation.
- **dhat bound:** extend `heap_allocation_bounds.rs` with a fat (~200-entry) string-table
  fixture and pin `max_blocks < baseline + num_strings` (not `+ 2*num_strings`) to catch
  a re-introduced double-parse.

### LOW

#### D4-01 — `bone_world` per-frame upload copies full `MAX_BONES_PER_MESH` stride per slot
- **Dimension:** SSBO Sizing & Upload · **Status:** Existing/residual (#1794, closed-with-residual — do **not** re-file)
- `crates/renderer/src/vulkan/scene_buffer/upload.rs:222-251`. The staging memcpy + GPU
  `cmd_copy_buffer` still cover `used_slots × 144 × 64 B` (≈58% padding for a 60-bone NPC).
  #1794 fixed the CPU re-identity-fill; narrowing the copy needs per-slot bone counts
  plumbed across the byroredux→renderer boundary (M29.5 variable-stride packing). Scales
  with used slots, not the ceiling. No new action.

#### D5-01 — Stale "volumetrics output is discarded / pure GPU waste" comment block
- **Dimension:** GPU Pipeline · **Status:** NEW · **Quick win**
- `crates/renderer/src/vulkan/context/draw.rs:659-704`. Comment says composite multiplies
  the volumetric result by 0.0 and the passes are "pure GPU waste." No longer true:
  `volumetrics.rs:163` sets `VOLUMETRIC_OUTPUT_CONSUMED = true` and `composite.rs:59-60`
  gates the multiplier to 1.0. Doc-only, but risks a future engineer "optimizing" away a
  now-live pass. Update the block.

#### D5-02 — `shadowableLightRadiance` re-derives L/dist/atten/NdotL the caller already has
- **Dimension:** GPU Pipeline · **Status:** NEW (leave as-is)
- `crates/renderer/shaders/include/lighting.glsl:78-110`, caller `triangle.frag:2277-2293`.
  A few redundant ALU ops per (light, fragment), bounded by cluster-culled light count.
  The duplication is a **deliberate** consequence of the #1369 bit-cancellation invariant
  (pass-1 accumulate and pass-2 shadow-subtract must evaluate a bit-identical expression).
  Leave unless a profiler shows the direct-lighting loop is ALU-bound; if revisited, pass
  `L`/`dist`/`atten` to **both** call sites together to preserve the invariant.

#### D6-01 — `skin.coverage` counters go stale on a bailed (early-return) frame
- **Dimension:** Skinning & BLAS · **Status:** NEW
- `crates/renderer/src/vulkan/context/draw.rs:2256` (coverage reset) sits AFTER the two
  early-return guards (`:2228`), unlike `skin_dispatch_ran` which is reset before them. A
  frame bailing through empty-framebuffers / `ERROR_OUT_OF_DATE_KHR` retains the previous
  frame's `SkinCoverageFrame`. Cosmetic (diagnostics during rare resize transients);
  optional fix is to move the coverage + descriptor-write resets above the guards.

#### D7-01 — Pending stream requests never cancelled when their cell leaves the load ring
- **Dimension:** Streaming & Cells · **Status:** NEW
- `byroredux/src/app_step.rs:87-161`, `streaming.rs:749-758`. `compute_streaming_deltas`
  derives `to_unload` from `state.loaded` only; a cell dispatched to the worker but not
  yet spawned is invisible to the unload diff. If the player leaves before it finishes,
  the payload still classifies `Apply` and pays a full main-thread spawn (terrain + BLAS +
  upload) — then unloads next boundary. Bounded by `MAX_CELLS_SPAWNED_PER_FRAME` and
  self-correcting; efficiency-only, world stays consistent. **Fix:** in the diff step,
  drop `state.pending` entries whose coord is now `> radius_unload` from `player_grid` so
  the in-flight payload classifies `StaleNoPending` and is discarded before spawn.

#### D8-02 — dhat geometry bound never exercises the packed-vertex allocation path it guards
- **Dimension:** NIF Parse · **Status:** NEW
- `crates/nif/tests/heap_allocation_bounds.rs:204-237,344-379`; guarded code
  `bs_tri_shape.rs:903-923` (`decode_bs_vertex_stream`). Both geometry fixtures build
  0-vertex blocks, so the six `allocate_vec(nv)?` output vecs + de-interleave loop — the
  actual #833/#831 site — never run under CI. A revert to `Vec::new()` + per-`push` growth
  would pass both gates. **Fix:** add a non-trivial `BSVertexDesc` fixture (~16 packed
  half-float verts) and pin `max_bytes < ceil(1.3 × Σ output-vec bytes)`.

#### D8-03 — Per-`NiParticleSystem` import does ~6 full `scene.blocks.iter()` scans
- **Dimension:** NIF Parse · **Status:** NEW (deferred, #1402)
- `crates/nif/src/import/walk/mod.rs:540-562`. O(K·B) for K particle systems; negligible
  at the K=1 vanilla common case, import-time only. Fold the color-curve / emitter-params
  / rate lookups into a single pass over `ps.modifier_refs` if multi-emitter attribution
  (#1402) lands. No action now.

#### D9-01 — CPU/GPU per-phase breakdown strings are `format!`-built every frame, not gated
- **Dimension:** Telemetry & Origin Cost · **Status:** NEW · **Quick win**
- `byroredux/src/systems/debug.rs:101-109` (`gpu_breakdown` 55-64 / `cpu_breakdown` 74-83).
  `log_stats_system` builds both `String`s unconditionally, but they're only read in the
  slow-frame branch or the 1 Hz boundary block — dropped on the overwhelming majority of
  frames. ~2 discarded `String` allocs/frame (~120/s at 60 fps). Erosion of the intended
  once-a-second gating. **Fix:** compute `let want = slow || crosses_one_second_boundary(..)`
  first, then build `gpu`/`cpu` only `if want`.

---

## Eroded guards

**None.** All Session-46 / Session-47 / interim regression guards verified intact with
file:line evidence across all 9 dimensions. D5-01 and D9-01 are doc/gating drift adjacent
to (not erosions of) their guards; D6-01 is a telemetry-accuracy gap, not a correctness
guard erosion.

---

## Prioritized Fix Order

**Quick wins (mechanical, no measurement needed):**
1. **D8-01** (MEDIUM) — `let bsver = scene.bsver;` one-liner; kills a per-NIF header
   re-alloc on the cell-load worker. Add the fat-string-table dhat fixture.
2. **D9-01** (LOW) — gate the two `format!` calls behind `want`; removes ~120 discarded
   String allocs/s.
3. **D5-01** (LOW) — update the stale volumetrics comment block (prevents a future
   regression that disables a live pass).
4. **D6-01** (LOW) — move the coverage/descriptor-write resets above the early-return
   guards for accurate `skin.coverage` on bailed frames.
5. **D8-02** (LOW) — add the non-zero packed-vertex dhat fixture so the #833/#831 geometry
   hygiene is CI-guarded, not grep-guarded.
6. **DIM2-02 / DIM2-03** (LOW, from Dim 2 report) — refresh stale "group by texture" and
   "Prospector ~811" comments in `render/mod.rs`.

**Measurement-gated (do not commit speculatively):**
7. **DIM2-01** (MEDIUM) — depth pre-pass. Real fragment-bound upside but needs RenderDoc
   confirmation the target cells are fragment-bound first.

**Deferred / architectural (already tracked):**
8. **D7-02** (MEDIUM) — interior sub-cell spawn budget (M40 follow-up).
9. **D7-01** (LOW) — pending-request cancellation on ring exit.
10. **D4-01** (LOW) — `bone_world` variable-stride narrowing (needs M29.5 boundary
    plumbing; do not re-file #1794).
11. **D5-02 / D8-03** — no action; documented invariant / deferred #1402.

---

*Generated by `/audit-performance` (deep, all 9 dimensions). Bench numbers cite ROADMAP
`8a668eff`; no bench re-run this session. Dedup incomplete (no `gh` access) — verify NEW
findings against live issues before filing.*
