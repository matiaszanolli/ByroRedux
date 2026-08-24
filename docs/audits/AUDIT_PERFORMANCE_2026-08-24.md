# Performance Audit — 2026-08-24

**Scope**: `/audit-performance` — all 9 dimensions, `--depth deep`, no `--focus`
filter (comprehensive run). Executed **solo** (single agent, no sub-agent
dispatch) per explicit instruction — the skill's own "Task agent (max 3
concurrent)" orchestration text was not followed.

**Repo state**: HEAD `048a8bd8`, branch `main`. 108 commits since the previous
performance sweep's baseline `bb0b92f2` (2026-08-20) — session-71 exterior
Tranche C/D (NAVM single-tile pathing wired into all 6 AI procedures, FormId→
Entity persistent-ref indices), WATAL convergence (buoyancy/current-force/
water-draw fixes), GPU morph-target blending (#3231), animated
alpha/color/shader/flipbook draw sinks (#2221), and a large ESM/scripting
batch (scene fragments, quest triggers, REGN ambient audio). Dedup baseline:
the 200-issue all-state fetch at `/tmp/audit/performance/issues.json` plus
`docs/audits/AUDIT_PERFORMANCE_2026-08-20.md` and its predecessors, and (new
for this cycle) two same-day sibling reports —
`docs/audits/AUDIT_ECS_2026-08-24.md` and `AUDIT_INCREMENTAL_2026-08-23.md` —
that already touch some of the same delta.

**Known limitation (per task instructions)**: a same-day audit found an
`E0004` non-exhaustive-match build break in
`crates/scripting/examples/fragment_coverage.rs:59` (an example target). No
`cargo` command was run for this audit regardless — every figure below is
derived from checked-in source, constants, struct layouts, and prior
bench-of-record data, consistent with this skill's existing "no cargo, no
engine instance" posture (`feedback_no_parallel_engine_launch.md`) — so the
break does not gate this report either way.

| Dim | Area | Findings |
|---|---|---|
| 0 | Skill-text / bench hygiene | **carried**: #3063, #3143 (both OPEN, not re-reported) |
| 1 | CPU Per-Frame Allocations & Hot Paths | **1 MEDIUM** (new) |
| 2 | Draw & Instancing | clean (guard intact; #3141 fix verified) |
| 3 | GPU Memory Pressure | clean (#3117 fix verified) |
| 4 | SSBO Sizing & Per-Frame Upload | **1 LOW** (new, doc drift) |
| 5 | GPU Pipeline & Pass Efficiency | clean (#3131 fix verified) |
| 6 | Skinning & BLAS Cost | **1 LOW** (new — morph-target sibling of #3061) |
| 7 | Streaming & Cells | **carried**: #3142 (OPEN, not re-reported) |
| 8 | NIF Parse | clean (guards intact, no new alloc sites of note) |
| 9 | Telemetry & Origin Cost | clean |

**0 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW** new findings this cycle, plus 3
carried OPEN issues (not re-reported) and 6 verified fixes.

---

## Executive Summary

**Every 08-20 finding has been triaged, and the ratio is good.** All six
findings from the prior sweep are resolved in code, with issues closed by
number: #3117 (`PERF-D3-01`, volumetrics VRAM ledger — corrected in
`docs/engine/memory-budget.md`), #3131 (`PERF-D5-01`, combustion transport
now gated on an active/lingering signal), #3133 (`PERF-D4-01`, fog cluster
offsets no longer re-derived/zero-filled per frame), #3135 (`PERF-D1-01`,
buoyancy fast path narrowed from "any wave amplitude" to "an actual nearby
contact" via `waves_require_contact_rescan`), #3137 (`PERF-D1-02`, water/
billboard collections converted to `FxHashSet`/`FxHashMap`, with a
source-text guard test), #3139 (`PERF-D1-03`, `WaterContactScratch` resource
now reuses `surfaces`/`current_volumes`/`targets`/`writes` via
`std::mem::take` + put-back instead of allocating fresh each frame), and
#3141 (`PERF-D2-01`, `reemit_water_planes` now indexes water draw slots
through an `FxHashMap<EntityId, usize>` instead of an O(N×W) linear scan).
All seven fixes were read directly at HEAD, not inferred from commit
messages alone.

**Two OPEN carries from 08-20 remain untouched and are not re-reported**:
#3142 (`PERF-D7-01`, `resident_vwd_refr_cells` still takes a fresh
`GlobalTransform` lock per VWD entity inside the LOD reconcile loop — code
unchanged at `byroredux/src/streaming_helpers.rs:295-307`) and #3143
(`PERF-D0-01`, this skill's own Dimension 5/7 checklist text still cites
superseded constants). #3143's underlying drift has in fact **widened**
since it was filed: `froxel_xy_divisor`'s default moved **4 → 8**
(`fc9e3e39`, "record the divisor's measured perceptual cost") after the 08-20
sweep read it as 4, so the skill text (still "default 12") is now two
generations behind rather than one. `STREAMING_APPLY_BUDGET` is unchanged at
16 ms (skill still says "4 ms"). Both are #3143's remit, not a new finding.

**Three new findings, all bounded.** The highest-value one (`PERF-D1-2026-
08-24-01`, MEDIUM) is a clean, mechanical fix: the six newly-wired AI-package
procedures (Travel/Guard/Escort/Follow, all landed this session via #2372)
each pay **two** full `VecDeque<Vec3>` clones per entity per tick — once
inside `resolve_cached_waypoints`'s cache-hit arm, once again to satisfy the
borrow checker before calling `step_along_waypoints` — where the second
clone is avoidable by restructuring the iteration to move rather than borrow.
The other two (`PERF-D6-2026-08-24-01`, `PERF-D4-2026-08-24-01`, both LOW)
are a new `std::collections::HashMap` sibling in the #3061 hot-path-hashing
cluster (GPU morph-target blending, #3231, landed after #3061 was filed) and
a two-document size drift (`shader-pipeline.md` + `memory-budget.md` both
still say `GpuMaterial` is 348 B; it grew to 364 B under #2221 and the code
guard is correct — only the authoritative-cited docs are stale, the same
drift class as the already-closed #3240 but a location that fix didn't
cover).

**One correctness-with-performance-shape finding from a sibling audit is
cross-referenced, not duplicated**: `AUDIT_ECS_2026-08-24.md`'s
`ECS-2026-08-24-09` (MEDIUM) found that an unrelated omnibus commit
(`4e1afcbe`) deleted the entire SpeedTree-geometry wind-bending loop from
`make_billboard_system` — the very code that carried #3193/#3192's "unreach-
able base cache" / "camera-parked early-out bypassed" findings. Those two
issues (both still OPEN) now describe **deleted** code; reconciling them is
this skill's Dimension 1/2 territory but the finding itself belongs to the
ECS audit that found the deletion, so it is not re-derived here. Flagged for
`/audit-publish` to cross-link rather than file a duplicate.

### Observed-vs-ROADMAP delta

None to report. ROADMAP's Bench-of-record is unchanged at `34074b93`
(2026-08-14) and, per ROADMAP's own text, is now **466 commits stale** (over
15× its 30-commit gate) — tracked as `R6a-stale-20` in ROADMAP's Known
Issues, not re-filed here. No bench was run this cycle (same posture as
08-20 and 08-16): every magnitude below is derived from checked-in constants,
struct layouts, and code paths, never a manufactured FPS figure.

### Hot-path cost table (derived from checked-in constants, not sampled)

| Signal | Source | Value |
|---|---|---|
| `froxel_xy_divisor` (default) | `crates/renderer/src/vulkan/upscaling.rs:135` | **8** (was 4 at 08-20's read; skill text still says 12 — see #3143) |
| `STREAMING_APPLY_BUDGET` | `byroredux/src/app_step.rs:33` | 16 ms (unchanged since 08-16; skill text still says 4 ms — see #3143) |
| `GpuInstance` size | `gpu_instance_is_160_bytes_std430_compatible` | 160 B — unchanged, morph-target address pair already accounted for |
| `GpuMaterial` size | `gpu_material_size_is_364_bytes` (`crates/renderer/src/vulkan/material.rs`) | **364 B** (grew from 348 B under #2221; `bindings.glsl` fixed by #3240, `shader-pipeline.md`/`memory-budget.md` NOT — see `PERF-D4-2026-08-24-01`) |
| `MAX_INSTANCES` / `MAX_INDIRECT_DRAWS` | `scene_buffer/constants.rs:140,163` | `0x40000` / aliased — unchanged |
| `MAX_MATERIALS` | `scene_buffer/constants.rs:192` | 16 384 — unchanged |
| `DRAW_SORT_PARALLEL_THRESHOLD` | `byroredux/src/render/mod.rs:564` | 3000 — unchanged |
| `PRE_PARSE_RAYON_MIN` | `byroredux/src/streaming.rs:1171` | 8 — unchanged |
| `ENABLE_LEGACY_WRS` | `shader_constants_data.rs:976`, generated `.glsl:251` | 0 in both — unchanged |

Per-pass GPU cost was **not** sampled this cycle either — `gpu_timers.rs` /
`ScratchTelemetry` are runtime-only and need a live Vulkan device plus
on-disk game data, unavailable in this environment
(`feedback_no_parallel_engine_launch.md`).

---

## Findings

### PERF-D1-2026-08-24-01: the six NAVM-pathed AI procedures each clone a full `VecDeque<Vec3>` waypoint list twice per entity per tick

- **Severity**: MEDIUM
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/navmesh_path.rs:343-362`
  (`resolve_cached_waypoints`'s cache-hit arm, `path.waypoints.clone()` at
  `:352`); consumers with the second clone:
  `byroredux/src/systems/travel.rs:231` (resolve) + `:262`
  (`p.waypoints.clone()`), `guard.rs:189` + `:223`, `escort.rs:239/278/300`
  (three call sites, mutually exclusive per tick) + `:340`, `follow.rs:216-221`
  + `:253` (`d.waypoints.clone()`)
- **Status**: NEW — the machinery is new this session (`#2372`, NAVM
  single-tile pathing wired into Travel/Guard/Escort/Follow this delta;
  Wander/Patrol are unaffected, see below)
- **Description**: `resolve_cached_waypoints` is the shared cache-lookup
  helper all six AI-package procedures call once their target/destination is
  known:
  ```rust
  match cached {
      Some(path) if path.goal.distance(goal) <= repath_threshold => {
          (path.goal, path.waypoints.clone())      // clone #1
      }
      _ => { /* recompute via path_from_resident_tiles */ }
  }
  ```
  For a frozen-goal caller (Travel, Guard's lead phase, Escort's lead phase)
  the threshold is `0.0` and the goal doesn't change tick-to-tick once
  resolved, so the cache-hit arm is the steady-state path for the entire
  travel/guard/escort leg — clone #1 fires every tick, not just on the first.

  Four of the six consumers (Travel, Guard, Escort, Follow) then pay a
  **second** clone to satisfy the borrow checker. Each builds its per-tick
  `TravelPending`/`GuardPending`/`EscortPending`/`FollowPending` scratch
  struct in an immutable-borrow pass (`for p in &scratch.pending`), then
  calls `step_along_waypoints(..., p.waypoints.clone(), ...)` because
  `step_along_waypoints` takes ownership (`mut waypoints: VecDeque<Vec3>`)
  and `p` is only borrowed:
  ```rust
  // travel.rs Pass 1b
  for p in &scratch.pending {
      let (new_pos, rotation, waypoints) = step_along_waypoints(
          p.current, p.rotation, p.waypoints.clone(), p.destination, dt,
          physics.as_deref(),
      );
      ...
  }
  ```
  So a travelling/guarding/escorting/following entity in its steady-state
  walking tick pays **two** full `VecDeque<Vec3>` allocations — one it
  didn't strictly need, since `scratch.pending` is itself per-frame scratch
  (`scratch.decisions.clear()` / rebuilt each call, matching the Session 46
  clear+extend pattern for the *outer* collection) whose entries are never
  read again after Pass 1b. Wander and Patrol don't have this shape: they
  resolve `waypoints` once and pass it directly into
  `step_oscillating_wander` (by value, no second pass, no clone).
- **Evidence**: verified all four double-clone sites read exactly as above
  at HEAD (`048a8bd8`); `escort.rs` has three `resolve_cached_waypoints`
  call sites but they are mutually exclusive per entity per tick (lead /
  fresh-transition-to-lead / collect), so the cost is still exactly two
  clones, not more. Waypoint counts are small (single-tile A* over one
  `NavmRecord`, shared-edge-midpoint extraction — typically single-digit
  entries), so each clone is a small heap allocation, not a large copy; the
  finding is about the churn shape (2 allocations × N active entities ×
  every tick), the same class Dimension 1's checklist names explicitly
  ("per-entity allocation inside a per-frame loop").
- **Impact**: Bounded by the population of entities actively running
  Travel/Guard/Escort/Follow with a live `Walking`/`Lead`/`Collect` state —
  all four procedures are opt-in behind their own env var
  (`BYRO_TRAVEL`/`BYRO_GUARD`/`BYRO_ESCORT`/`BYRO_FOLLOW`), none in the
  default scheduler, so this is not a default-configuration regression. It
  is a real, measurable, and easily-fixed inefficiency in code that exists
  specifically to make those procedures reachable at all — the double-clone
  pattern will scale directly with however many actors end up running these
  packages once they're enabled by default. There is **no quantitative
  allocation guard for this site** (dhat is process-singleton, per this
  skill's Regression-Guard Posture).
- **Related**: `AUDIT_ECS_2026-08-24.md`'s `ECS-2026-08-24-08` examines the
  same function/call sites for a cache-invalidation correctness gap
  (residency epoch) — different defect, same location, cross-referenced
  rather than duplicated.
- **Suggested Fix**: Drop the second clone by restructuring Pass 1b to
  consume `scratch.pending` by value — `for p in scratch.pending.drain(..)`
  (or `std::mem::take(&mut scratch.pending).into_iter()`) instead of `for p
  in &scratch.pending` — since nothing reads `scratch.pending` after Pass 1b
  in any of the four systems. That removes clone #2 entirely with no
  behavior change. Clone #1 (inside `resolve_cached_waypoints`) is
  structurally harder to remove without changing `NavPath`'s storage shape
  (it's a component read, and the caller needs an owned `VecDeque` to
  eventually write back via `pop_reached_waypoint`) — leave it, but note the
  fix above halves the cost for the four affected systems.

---

### PERF-D6-2026-08-24-01: GPU morph-target blending's `MorphSlot` map lands on `std::collections::HashMap`, and its per-frame weight upload allocates one `Vec<f32>` per entity — both are new siblings of the open #3061 hot-path-hashing cluster

- **Severity**: LOW
- **Dimension**: Skinning & BLAS Cost (closest fit — GPU morph-target
  blending is the deformation-pipeline sibling of skinning, landed by #3231
  after #3061 was filed, so it's not a #3061 duplicate but the same class)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1393-1396`
  (`pub morph_slots: std::collections::HashMap<EntityId, MorphSlot>`);
  `crates/renderer/src/vulkan/context/draw.rs:3011` (per-draw-instance
  `self.morph_slots.get(&draw_cmd.entity_id)`); `byroredux/src/render/
  skinned.rs:280-295` (`update_morph_weights` — `ctx.morph_slots.iter_mut()`
  every frame, plus `let flat: Vec<f32> = (0..target_count).map(...).collect();`
  per entity at `:290`)
- **Status**: NEW — `morph_slots` was added by #3231, landed in this delta,
  after #3061 (filed from the 08-16 audit) enumerated its six then-existing
  siblings. #3061 does not and could not have covered this field.
- **Description**: Two related but distinct costs on the same per-frame
  path:
  1. `morph_slots` is a plain `std::collections::HashMap<EntityId,
     MorphSlot>` — SipHash-1-3 on an `EntityId` key, looked up once per
     morphed entity per draw (`draw.rs:3011`) and iterated in full every
     frame by `update_morph_weights` (`skinned.rs:285`). This is exactly
     the access shape #2923 converted for `pose_dirty` and #3061 is tracking
     as the unfinished residual for `skin_slots` and five siblings — one
     more instance of the same cluster, introduced after that issue was
     filed.
  2. `update_morph_weights`'s per-entity body (`skinned.rs:290`) builds a
     fresh `Vec<f32>` via `.collect()` every frame for every live
     `MorphSlot` that also has an `AnimatedMorphWeights` component — bounded
     (`≤ MAX_MORPH_TARGETS_PER_MESH` × 4 B ≈ 256 B per entity) but still a
     heap allocation inside a per-entity per-frame loop, the pattern
     Dimension 1's checklist names directly. The function's own doc
     justifies the *unconditional write* (no `pose_dirty`-style skip gate,
     "far cheaper than the dirty-tracking bookkeeping would save") but does
     not address the allocation itself — `MorphSlot::update_weights` only
     needs a `&[f32]`, so the collect could write into a small reused
     scratch buffer (or, given the bound is ≤64 floats, a fixed-size stack
     array) instead of a fresh `Vec`.
- **Evidence**: both sites read directly at HEAD; `skin_slots` (the #3061
  original) sits one struct field above `morph_slots` in the same file,
  confirming the new field did not inherit the Fx conversion the cluster is
  working toward.
- **Impact**: Small and bounded — SipHash over a per-frame keyspace sized to
  the count of skinned-and-morphed entities (a subset of skinned entities,
  itself already bounded by `SkinSlotPool`), plus one small allocation per
  such entity per frame. Not a leak, not DoS-facing, not currently a hot-
  path bottleneck on any measured scene. Reported because it is precisely
  the drift #3061 exists to prevent recurring, and because leaving it
  unflagged lets the cluster grow to 7-of-10 instead of the "6 of 9" #3061
  already documents.
- **Related**: #3061 (`PERF-D6-01`, OPEN) — this is a new sibling instance
  in the same cluster, not a separate defect; recommend folding into that
  issue's scope rather than filing a seventh near-duplicate ticket.
- **Suggested Fix**: Convert `morph_slots` to `FxHashMap<EntityId,
  MorphSlot>` alongside whatever pass resolves #3061 (same mechanical
  change, same guard-assertion extension). For the per-entity allocation,
  either reuse a `Vec<f32>` scratch buffer captured by
  `update_morph_weights`'s caller (clear+extend per entity, matching the
  `AnimScratch` pattern) or, since `target_count` is bounded by
  `MAX_MORPH_TARGETS_PER_MESH`, write into a fixed-size
  `[f32; MAX_MORPH_TARGETS_PER_MESH]` and slice it — no heap allocation at
  all.

---

### PERF-D4-2026-08-24-01: `docs/engine/shader-pipeline.md` and `docs/engine/memory-budget.md` — the two authoritative docs this skill cites for GPU struct sizing — still state `GpuMaterial` is 348 B; it grew to 364 B under #2221

- **Severity**: LOW
- **Dimension**: SSBO Sizing & Per-Frame Upload
- **Location**: `docs/engine/shader-pipeline.md:283` (section header
  `### GpuMaterial — 348 bytes...`), `:326` (`total **348**`), `:330`
  (`from 300 B to 348 B`), `:395` (`348 B each; deduplicated per frame`);
  `docs/engine/memory-budget.md:34` (`Material SSBO | ... | 348 B | 5.7 MB |
  11.4 MB`); ground truth `crates/renderer/src/vulkan/material.rs`
  (`gpu_material_size_is_364_bytes`, `shader_color_r`/`shader_float` at
  offsets 348/360 → total 364) and `crates/renderer/shaders/include/
  bindings.glsl:99,107-108` (already correct — fixed by #3240)
- **Status**: NEW — a different pair of locations than #3240 (CLOSED), which
  fixed only `bindings.glsl`'s comment
- **Description**: #2221 (landed this delta) added `shader_color_r/g/b` +
  `shader_float` to `GpuMaterial` for the animated-shader-property sinks,
  growing it from 348 B to 364 B — correctly, with the Rust-side test
  renamed to `gpu_material_size_is_364_bytes` and `bindings.glsl`'s own doc
  comment updated in the same commit. `docs/engine/shader-pipeline.md` (the
  doc this skill's Key Reference Docs table names as "`GpuInstance`/
  `GpuMaterial` exact byte layouts") and `docs/engine/memory-budget.md` (the
  doc Dimension 4's checklist explicitly says to cite rather than
  re-derive) were not updated — both still describe the pre-#2221 348 B
  struct, and `memory-budget.md`'s Material SSBO row (`16 384 × 348 B =
  5.7 MB`) is now computed off the stale size.
- **Evidence**: `gh issue view 3240` confirms its scope was exactly
  `bindings.glsl:99,107-108` and explicitly notes *"Not covered by open
  #2483, which lists only `gpu_types.rs:84` and `constants.rs:168`"* — this
  finding's two locations are a third, fourth, and fifth site the existing
  tracking issues do not cover.
- **Impact**: Documentation-only — the code guard (`gpu_material_size_is_
  364_bytes`) is correct and would fail CI if the struct drifted, so there
  is no runtime corruption risk. The practical effect is small (16 384 ×
  16 B = 256 KB understatement on the Material SSBO row, negligible against
  the multi-GB budget) but this is exactly the failure mode
  `_audit-common.md`'s Path-Reference Convention section calls out by name
  (the `GpuMaterial` 300→348 B example it already uses to justify the
  symbol-advisory check) — the doc that teaches auditors to catch this drift
  class is itself now citing a number one generation behind.
- **Related**: #3240 (CLOSED, same drift class, `bindings.glsl` only).
- **Suggested Fix**: Update `shader-pipeline.md:283,326,330,395` and
  `memory-budget.md:34` to 364 B, recompute the Material SSBO row (16 384 ×
  364 B = 5.97 MB), and note the two-step growth (300 → 348 → 364) the way
  `bindings.glsl`'s comment now does.

---

## Prioritized Fix Order

1. **PERF-D1-2026-08-24-01** — mechanical, no behavior change: switch Pass
   1b's four call sites from `for p in &scratch.pending` to a draining/
   owning iteration. Removes one full `VecDeque<Vec3>` clone per entity per
   tick in Travel/Guard/Escort/Follow.
2. **PERF-D4-2026-08-24-01** — a doc edit (5 line-ranges across 2 files),
   but it's the exact drift class `_audit-common.md` singles out by name;
   cheap to fix and immediately closes the "docs teaching auditors to catch
   this are themselves stale" irony.
3. **PERF-D6-2026-08-24-01** — fold into whatever pass resolves #3061;
   don't fix in isolation, since the whole point of that issue is doing the
   Fx conversion once across the full cluster.
4. Carried, not re-proposed here: #3142 (`resident_vwd_refr_cells` lock
   hoist), #3143 (skill-text numeric drift — now also covering the
   `froxel_xy_divisor` 4→8 move that happened after #3143 was filed).

---

## Guards verified intact (do NOT re-propose)

Re-verified at HEAD `048a8bd8`, same guard set as the 08-20 sweep plus the
three fixes it identified as needed:

- **Dim 1**: `drain_dirty_into` sole production drain path (`take_dirty`'s
  only non-test callers are inside `#[test] fn` bodies in `billboard.rs`,
  verified by reading the surrounding function, not just grep) · `entities_
  scratch`/`playback_scratch` clear+extend (`systems/animation.rs:496-497,
  583-584`) · billboard `last_cam` gate (`systems/billboard.rs:26,81,85`) ·
  **new this cycle**: `WaterContactScratch` reuse in `apply_buoyancy`
  (`crates/physics/src/water.rs:536-568`, `std::mem::take` + put-back) and
  `waves_require_contact_rescan` narrowing the fast-path gate to actual live
  contacts (`water.rs:490-508`) — both confirm #3135/#3139's fixes are real,
  not just closed-by-claim.
- **Dim 2**: `DRAW_SORT_PARALLEL_THRESHOLD = 3000` (`render/mod.rs:564`) ·
  `needs_two_sided_blend_split` still has no `z_write` limb
  (`context/draw.rs:1341`) · **new this cycle**: `reemit_water_planes`
  indexes through `FxHashMap<EntityId, usize>`
  (`byroredux/src/render/water.rs:29,37,115`), confirming #3141's fix.
- **Dim 3**: dynamic BLAS budget + `blas_over_budget`
  (`acceleration/predicates.rs:470,667`) — unchanged.
- **Dim 4**: `MAX_INSTANCES = 0x40000`, `MAX_MATERIALS = 16384`
  (`scene_buffer/constants.rs:140,192`), `MAX_INDIRECT_DRAWS` aliased
  (`:163`) — unchanged. `GpuInstance` still 160 B including the #3231 morph
  address pair (see `PERF-D4-2026-08-24-01` for the sibling `GpuMaterial`
  doc gap, which is *not* a code drift).
- **Dim 5**: `ENABLE_LEGACY_WRS = 0` in both the Rust source and the
  generated `.glsl` (`shader_constants_data.rs:976`,
  `include/shader_constants.glsl:251`) — unchanged. **New this cycle**:
  combustion transport now gates on an active/lingering signal (#3131,
  verified via the commit's own description of `frame_params.fog_reference[
  3]`; not independently re-read shader-side given budget — flagged as
  commit-message-sourced, lower confidence than the Rust-side checks above).
- **Dim 7**: `PRE_PARSE_RAYON_MIN = 8` (`streaming.rs:1171,1183`) —
  unchanged.
- **Dim 8**: `read_pod_vec` sole bulk reader (`nif/src/stream.rs:438`);
  spot-checked new morph-import code
  (`crates/nif/src/import/mesh/morph.rs`) uses `Vec::with_capacity` at the
  one allocation site of note (`:73`) — load-time, correctly sized, not
  dhat-bound yet but not a regression either.
- **Dim 9**: `origin_corrected_prev_view_proj` present
  (`context/draw.rs:4046`) — unchanged.

---

## Existing OPEN issues touched (deduplicated, not re-reported)

#3142 (`PERF-D7-01` — `resident_vwd_refr_cells` per-entity lock; unchanged,
re-verified at HEAD) · #3143 (`PERF-D0-01` — skill-text constant drift; the
underlying `froxel_xy_divisor` value has moved again, 4→8, since this issue
was filed) · #3061 (`PERF-D6-01` — hot-path-hashing cluster; `morph_slots`
is a new, uncounted sibling — see `PERF-D6-2026-08-24-01`) · #3122
(`PHYS-D6-2026-08-20-04`, physics-domain, OPEN — describes the buoyancy fast
path as unreachable in the shipping binary; re-reading `water.rs` at HEAD
suggests the same fix that closed this skill's own #3135
(`waves_require_contact_rescan`) also resolves #3122's premise, since the
gate no longer keys on wave amplitude alone but on live nearby contacts —
**not verified independently against #3122's own repro**, flagged for
`/audit-physics` or a direct re-check rather than closed here) · #3193 /
#3192 (SpeedTree geometry-wind findings from the physics/speedtree domain —
now describe code `ECS-2026-08-24-09` found **deleted** by an unrelated
commit; cross-domain reconciliation needed, not attempted here).

## Fixed since the last sweep (verified in code, not just by commit message)

#3117 (`PERF-D3-01`) · #3131 (`PERF-D5-01`, verified via commit description
only — see Dim 5 guard note above) · #3133 (`PERF-D4-01`) · #3135
(`PERF-D1-01`) · #3137 (`PERF-D1-02`) · #3139 (`PERF-D1-03`) · #3141
(`PERF-D2-01`).

## Scope note

Given the 108-commit delta's size, this cycle prioritized (a) verifying
every 08-20 finding's disposition against HEAD, (b) sweeping the file list
touched since 08-20 for new per-frame allocation/hashing patterns in the
areas this skill's dimensions cover, and (c) spot-checking the largest new
subsystems (GPU morph-target blending #3231, NAVM pathing #2372, animated-
material draw sinks #2221). It did **not** exhaustively re-trace every
dimension from first principles the way a from-scratch sweep would — Dims 2,
3, 5, 8, 9 are reported "clean" on the strength of guard re-verification
plus no new code surfaced in their entry-point files, not a fresh full
re-read of `draw.rs`/`volumetrics.rs`/`stream.rs` end to end. The large
scripting/ESM batch this delta (scene fragments, quest triggers, REGN
ambient audio, FormId→Entity indices) was spot-checked for obviously
unbounded per-frame patterns (none found: `trigger_detection_system`'s
allocations are bounded by `TriggerVolume` count, which is small and
quest-specific) but not traced in the depth `/audit-scripting` or
`/audit-esm` would apply — those remain the owner audits for correctness in
that domain. No `cargo` command was run and no engine instance was launched,
consistent with prior cycles.

TALLY: CRITICAL=0 HIGH=0 MEDIUM=1 LOW=2
