# ByroRedux Tech-Debt Audit — 2026-08-03

Comprehensive-suite leg. Depth: deep (per-instance triage with concrete fix
proposals across all 9 dimensions). Prior report:
`docs/audits/AUDIT_TECH_DEBT_2026-08-02.md`.

## Session Context

28 commits landed between the prior report and this one (`e4b8b1a1`..`HEAD`),
inside less than 24 hours — a very fast turnaround. Two things happened in
that window:

1. **Five direct fixes for yesterday's tech-debt findings** landed and are
   confirmed holding: TD1-081/`build_tlas` (#2259), TD1-080/`record_post_passes`
   (#2258), TD1-NEW-02/`draw_frame` re-growth (#2255), TD2-101/cinematic.rs
   duplication (#2260), and TD4-001/crate-roster staleness (#2261). TD3-201/
   202/203 (doc rot) were also fixed the same day, just before this window's
   `e4b8b1a1` cutoff, and are re-verified below.
2. **A large Session-63 renderer/NIF feature push** (18 more commits) covering:
   shadow-projection canonicalization (#2250/#2251), tangent-reconstruction
   correctness fixes (#2245/#2246), Cornell RT-harness fog/fire-refraction
   coverage (#2248/#2249), egui skipped-frame texture-delta merging (#2247),
   packed-mesh winding vs. Oblivion's authored normals (#2193), NiBillboardNode
   propagation (#2206), NIF light kind/direction/cone wiring (#2205),
   non-finite shader diagnostics (#2218/#2227), a `GpuFogVolume` GLSL lockstep
   test (#2228/#2231), height-fog ground anchoring (#2225), a stale
   `SkyParamsRes` leak fix (#2226), and renderer-audit-driven fixes
   (#2234-2237). The correctness side of this push was covered by same-day
   renderer/NIF audits; this report covers the *debt* left in its wake.

Each of the 9 dimensions was re-run as an independent delta check against
yesterday's clean baseline rather than a from-scratch sweep, since the prior
cycle was thorough and same-day.

## Executive Summary

**7 NEW findings** (0 CRITICAL, 0 HIGH, 2 MEDIUM, 5 LOW), plus **12 existing
OPEN items re-verified accurate and still open** (not re-numbered; listed
under "Still Open, Re-Verified" per dimension below). This is a smaller yield
than yesterday's 19, consistent with the fact that yesterday's audit was
same-day and already caught most of the surface — today's task was mostly
confirming holds and hunting for debt introduced by the follow-on Session-63
push.

**Dimensions 5, 6, 7, 8, 9 are entirely clean** this cycle: no new findings,
and in three cases (Dim 5 markers, Dim 8 dead-code set, Dim 9 `#[ignore]` set)
the finding population was verified **byte-identical** to yesterday's, not
just coincidentally equal in count. Dimension 6 (stub implementations) is
still 0 `unimplemented!()`/`todo!()`/`panic!("not …")` repo-wide, now
including a first-time-scoped check of the Cornell RT harness's new fog/
fire-refraction coverage and the new shadow-flag canonicalization boundary —
both are genuine, fully-wired implementations, not stubs, despite the
brief's specific concern that they might be.

**All 2 MEDIUM findings are Dimension 3 (doc rot)**, and both are the *exact
same failure mode* the last three audits already diagnosed: fast renderer
work outpacing the docs describing its GPU contract:
- **TD3-204**: `docs/engine/renderer.md` still quotes `GpuInstance` at 112 B
  and `GpuMaterial` at 300 B — both grew (128 B / 348 B) in commits this file
  was *not* touched by, even though the sibling `shader-pipeline.md` and every
  `audit-*/SKILL.md` were correctly updated.
- **TD3-205**: yesterday's own TD3-202 fix (`feature-matrix.md`'s Fire
  refraction row, landed `b9a68eea`) went stale within about six hours —
  it cites #2236/#2237 as open "consistency gaps," but both closed the same
  day via `cd6a8338`. This is now the **4th recurrence** of the
  feature-matrix-lags-shipped-code pattern across four consecutive cycles
  (07-16, 07-25, 08-02, 08-03), and this time the *fix itself* was the thing
  that went stale, not the pre-existing content — worth flagging as the
  pattern outrunning even same-day remediation.

Dimension 1 (complexity) produced 3 new LOW findings, all small/diffuse
growth rather than a new giant function: `collect_lights` crossed 200 LOC
(179→208), the newly-2000-LOC `crates/nif/src/import/tests.rs` is a single
+112-line test-only commit (low risk, same shape as prior resolved
crossings), and the Cornell test harness's `setup_cornell_scene` grew to 296
LOC. `draw.rs`/`mod.rs`'s further growth (+229/+93 LOC) was investigated and
is **diffuse across three unrelated small fixes**, not a resurgent monolith —
`draw_frame` (2052 LOC) and `VulkanContext::new()` (1205 LOC) both held flat.

Dimension 2 produced 1 new LOW finding (TD2-115): the bitangent-sign clamp
idiom is now duplicated across 4 sites (2 files) rather than sharing one
helper — cosmetic today, same divergent-fix-history risk class the project's
already-closed TD2-001 was fixed for.

## Baseline Snapshot (for the next audit to diff)

```
TODO/FIXME/HACK/XXX:    19   (unchanged — verified byte-identical set to 08-02, not
                              just same count; none of the 7 marker-bearing files
                              were touched by any of the 28 commits)
allow(dead_code):       39   (unchanged — verified byte-identical set to 08-02 via
                              detached-worktree diff)
unimplemented!/todo!(): 0    (unchanged, including a first-time check of the new
                              Cornell fog/fire-refraction coverage and the new
                              shadow-flag canonicalization boundary — both real)
#[ignore] tests (.rs-scoped): 135  (unchanged — verified byte-identical set to 08-02;
                              two files each gained one unrelated line near an
                              existing #[ignore], net membership swap = zero)
files >2000 LOC:        5    (was 4 on 08-02 — set changed, not just regrew)
path gate:              GREEN (1167 refs / 26 skill files, 0 stale)
```

Oversized set (live, today):
```
4027  crates/renderer/src/vulkan/context/draw.rs        (TD1-NEW-02, no GH issue —
                                                            grew 3798→4027; draw_frame itself
                                                            HELD at 2052 LOC, growth is 3 diffuse
                                                            unrelated small fixes: #2219 skinned-RT
                                                            hit-normal, #2225 height-fog anchor,
                                                            #2247 egui texture-delta merge)
4010  crates/renderer/src/vulkan/context/mod.rs         (Existing: #1749, OPEN — grew 3917→4010;
                                                            new() HELD at 1205 LOC, file growth is
                                                            diffuse, same three commits as above)
2165  crates/renderer/src/vulkan/volumetrics.rs          (Existing: #2256, OPEN — grew 2075→2165;
                                                            new_inner HELD at 555 LOC, growth is new
                                                            tests only)
2030  crates/nif/src/import/tests.rs                     (TD1-083, NEW — crossed via a single
                                                            +112-line commit, #2206's 3 new
                                                            billboard-mode regression tests; 100%
                                                            #[cfg(test)] content)
2015  crates/renderer/src/vulkan/material.rs              (Existing: #2257, OPEN — untouched this
                                                            window, unchanged at 2015)
```

## Verified Fixed From Prior Cycle

- **TD1-081 / #2259** (`build_tlas`): CONFIRMED. Split into `build_tlas_instances` +
  `ensure_tlas_state`, real decomposition (not a rename). `tlas.rs` now 944 LOC
  (was ~880 pre-split as one function).
- **TD1-080 / #2258** (`record_post_passes`): CONFIRMED. Split into 9 named
  `record_<pass>_pass` helpers, call order preserved. `post_passes.rs` now 893 LOC.
- **TD1-NEW-02 / #2255** (`draw_frame` re-growth): CONFIRMED. `build_composite_params`
  extracted as specified; `draw_frame` holds at 2052 LOC and has **not** regrown
  since — the file's further growth this window is unrelated diffuse code (see
  Baseline Snapshot above).
- **TD2-101 / #2260** (`cinematic.rs` duplication): CONFIRMED. Generic
  `sample_keyed<T, V>(keys, time, default, time_of, value_of, lerp)` helper now
  backs both `sample_scalar`/`sample_color`; shared control flow lives exactly
  once, with 2 dedicated tests.
- **TD3-201** (`shader-pipeline.md` GpuLight byte table): CONFIRMED live and correct.
- **TD3-202** (`feature-matrix.md` Volumetrics row): CONFIRMED live and correct
  (but see TD3-205 below — a *different* row this same fix touched has since
  gone stale).
- **TD3-203** (`ROADMAP.md` Tier 8 / M55 contradiction): CONFIRMED live and correct.
- **TD4-001 / #2261** (crate roster): CONFIRMED. `_audit-common.md` now says
  "23 crates" and lists `hkx`; matches `ls crates/` 1:1.

## Verified Still Open (re-checked accurate, not re-numbered)

- **Existing: #1749** — `VulkanContext::new()`, still 1205 LOC, unchanged this window.
- **Existing: #2256** — `volumetrics.rs` `new_inner`, still 555 LOC, unchanged.
- **Existing: #2257** — `material.rs`, still 2015 LOC, file untouched this window.
- **Existing: #2262 (TD4-002)** — skill's own `#[ignore]`-grep recipe still
  unfiltered at both call sites (`audit-tech-debt/SKILL.md:71` and `:291`);
  reproduced the false signal live: 333 unfiltered vs. 135 `.rs`-scoped (2.47x,
  matching the issue's own estimate). Not touched by any of today's commits,
  including the otherwise-broad `1ae86f62` SKILL.md refactor pass.
- **Existing: #2263 (TD5-001)** — XXXX-protocol exclusion list still doesn't
  name `esm/cell/wrld.rs`/`esm/cell/mod.rs`; both lines still present as described.
- **Existing: #2264 (TD6-001)** — `ROADMAP.md:758` still calls PACK/QUST/DIAL/
  MESG/PERK/SPEL/MGEF "stubs (#446/#447)" (shifted one line by unrelated edits).
- **Existing: #2265 (TD7-001)** — the 8-layer ray-walk budget is still
  hand-declared independently at all 3 sites (`raytrace.glsl:64`,
  `water.frag:256`, `shadow_transport.glsl:11`); none touched this window.
- **Existing: #2266 (TD8-001)** — orphaned NPC-spawn sync wrappers, unchanged,
  zero call sites.
- **Existing: #2267 (TD8-002)** — unused `hkx` `global_target` accessor, unchanged.
- **Existing: #2268 (TD8-003)** — dead NIF particle-modifier shims, unchanged.
- **Existing: #2076 (TD8-102)**, **#1762 (TD8-005)**, **#1761 (TD8-004)** —
  spot-checked accurate, none of their files touched this window.

## Top 10 Quick Wins

1. **TD3-204** (trivial) — fix `docs/engine/renderer.md`'s two stale GPU-size
   quotes (GpuInstance 112→128 B, GpuMaterial 300→348 B).
2. **TD3-205** (trivial) — fix `feature-matrix.md`'s Fire-refraction row:
   #2236/#2237 are now CLOSED, not open "consistency gaps."
3. **TD2-115** (small) — add a shared `clamp_sign(f32) -> f32` helper beside
   `bitangent_sign` in `crates/nif/src/types.rs`, call it from `bs_geometry.rs`'s
   #2246 fix (currently 4 duplicate ternary sites across 2 files).
4. **TD1-083** (trivial, deferrable) — split `crates/nif/src/import/tests.rs`
   (2030 LOC, 100% test content) into per-topic siblings (transform /
   material_texture / bs_subclass / particle / furniture / billboard),
   mirroring the already-closed `anim/tests/` precedent.
5. **TD1-082** (trivial) — `collect_lights` (`render/lights.rs:106`, 208 LOC)
   crossed the 200-LOC function-size guideline; extract the per-light-kind
   dispatch into a helper if it grows further.
6. **TD1-084** (trivial, deferrable) — `setup_cornell_scene` (test harness,
   296 LOC) is a low-priority split candidate; not load-bearing production code.
7. **#2262** — rescope the tech-debt skill's own `#[ignore]`-grep recipe to
   `--include='*.rs'` at both Phase 1 and Dimension 9 (still not fixed, one
   cycle later; the false 2.47x signal is now reproducible on demand).
8. **#2263** — extend the `XXXX`-protocol exclusion list to name
   `esm/cell/wrld.rs`/`esm/cell/mod.rs`, or key it on content instead of a
   file-path enumeration so it stops going stale every time a new legitimate
   site is added.
9. **#2264** — `ROADMAP.md:758`, replace "stubs (#446/#447)" with "now fully
   parsed" — 103 days stale.
10. **#2265** — consolidate the 8-layer ray-walk budget (`MAX_TRANSPARENT_SKIPS`
    ×2, `MAX_OPAQUE_LAYERS` ×1) into one `shader_constants_data.rs` constant.

*(Also worth doing opportunistically: **#2266/#2267/#2268** — three small
dead-code deletions (~30 min combined), all still open one cycle later with
no blocker.)*

## Top 5 Medium Investments

Unchanged from yesterday's list — no new medium/large item surfaced this
cycle, and none of the prior list's items were touched:

1. **Existing #1749** — `VulkanContext::new()` (1205 LOC). Only phase 1
   (`build_core_device`) of the issue's 4-phase plan has landed.
2. **Existing #2256** — `volumetrics.rs` (2165 LOC), split along the
   `new_inner`/`create_volume`/`initialize_layouts` vs. `dispatch`/`write_*`
   boundary, mirroring `context/{mod,draw,resize,...}.rs`.
3. **`draw_frame` standing tracking issue** — still recommended: two
   extraction rounds (#2255 and its 07-25 predecessor) have each held only
   days to weeks against file-level (not function-level) regrowth; a
   lightweight tracking issue would make this visible between cycles instead
   of independently rediscovered each time. `draw_frame` itself is currently
   holding flat, so this is process advice, not an active complaint.
4. **TD1-083** — `crates/nif/src/import/tests.rs` split (medium only because
   of the number of sibling files to create; each individual move is
   mechanical and low-risk).
5. **Existing #2257** — `material.rs` test-module extraction into
   `material_tests.rs`, mirroring the `texture_registry.rs` precedent
   (trivial effort, listed here only because it's been open across 2 cycles
   with a one-line fix).

## Findings

### MEDIUM

#### TD3-204: docs/engine/renderer.md quotes stale GpuInstance (112 B) / GpuMaterial (300 B) sizes
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/engine/renderer.md:129-130`, `:499`
- **Status**: NEW
- **Description**: `GpuInstance` grew 112→128 B via commit `4ddf754a` (#2219,
  reconstructing skinned RT hit normals from deformed geometry) and
  `GpuMaterial` grew 300→348 B via an earlier commit (`1d94eb24`). Both
  `docs/engine/shader-pipeline.md`, every `audit-*/SKILL.md`, and
  `_audit-common.md` were correctly updated at the time — `renderer.md` was
  not. `#2219`'s own commit patched a *different* GpuInstance mention 30 lines
  later in the same file but skipped these two, and never touched
  `GpuMaterial` here at all even though the sibling docs got that update via
  an unrelated commit that also never touched `renderer.md`.
- **Evidence**: `docs/engine/renderer.md:129-130` and `:499` (112 B / 300 B)
  vs. `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`
  (`gpu_instance_is_128_bytes_std430_compatible`, `gpu_material_size_is_348_bytes`
  in `material.rs:1272`) — both live at 128 B / 348 B.
- **Impact**: `#[repr(C)] GPU struct size/layout drift` is a HIGH-minimum
  category per `_audit-severity.md` when it's a live code drift; here the code
  is correct and only the doc is stale, which is why this stays MEDIUM (per
  the tech-debt promotion table: "stale `GpuCamera`/`GpuInstance`/`GpuMaterial`
  size in a doc comment" floors at MEDIUM) rather than HIGH. Still misleads
  any reader of `renderer.md` specifically, which is otherwise treated as
  authoritative.
- **Related**: Same failure class as the already-fixed TD3-NEW-01 (Vertex
  byte size) and TD3-201 (GpuLight fields) — third recurrence of this exact
  doc, different fields, one cycle later.
- **Suggested Fix**: Update both call-sites to 128 B / 348 B, matching
  `shader-pipeline.md`'s wording.
- **Age**: `GpuInstance` drift is 1 day old (`#2219` landed today); `GpuMaterial`
  drift predates this window.
- **Effort**: trivial

#### TD3-205: feature-matrix.md's Fire-refraction row (yesterday's own fix) cites #2236/#2237 as open — both closed same day
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/feature-matrix.md:51`
- **Status**: NEW
- **Description**: Yesterday's TD3-202 fix (`b9a68eea`) added a Fire
  refraction row citing #2236/#2237 as "known consistency gaps." Both issues
  were closed the same day via `cd6a8338` ("Fix #2234, #2235, #2236, #2237:
  renderer audit findings"), confirmed via `gh issue view` (`closedAt:
  2026-08-03T02:06Z` for both). The row went stale within roughly six hours
  of being written — the fastest doc-rot turnaround observed across any
  cycle of this audit.
- **Evidence**: `docs/feature-matrix.md:51`; `gh issue view 2236 2237 --json state,closedAt`.
- **Impact**: This is the **4th consecutive cycle** (07-16, 07-25, 08-02,
  08-03) the exact same feature-matrix-lags-shipped-code pattern has
  recurred — see "Recurring Pattern" note below. Worth treating as a process
  gap rather than four independent doc bugs.
- **Related**: TD3-101, TD3-NEW-03, TD3-202 (all closed, same file, same
  pattern, third-through-fourth recurrence).
- **Suggested Fix**: Remove or update the "known consistency gaps" clause to
  reflect #2236/#2237 as resolved; if any residual gap remains, name it
  specifically rather than by now-closed issue number.
- **Age**: ~18 hours.
- **Effort**: trivial

### LOW

#### TD1-082: collect_lights crossed 200 LOC
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `byroredux/src/render/lights.rs:106` (`collect_lights`, 179→208 LOC)
- **Status**: NEW
- **Description**: Grew past the skill's 200-LOC function-size guideline via
  this window's light-kind/shadow-flag wiring (#2205, #2250/#2251). Not yet
  large enough to be a standalone medium-effort item.
- **Suggested Fix**: If it grows further, extract the per-light-kind (point/
  spot/directional) dispatch arms into named helpers.
- **Age**: this window.
- **Effort**: trivial (watch-and-wait; no action strictly required yet)

#### TD1-083: crates/nif/src/import/tests.rs newly crossed 2000 LOC
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/nif/src/import/tests.rs` (2030 LOC)
- **Status**: NEW
- **Description**: Crossed via a single +112-line commit (#2206, 3 new
  `NiBillboardNode` mode regression tests). 100% `#[cfg(test)]` content, zero
  production code — same low-risk shape as prior resolved crossings
  (`anim/tests.rs`, `material.rs`).
- **Suggested Fix**: Split into per-topic siblings: `transform.rs`,
  `material_texture.rs`, `bs_subclass.rs`, `particle.rs`, `furniture.rs`,
  `billboard.rs`, mirroring the already-closed `anim/tests/` precedent.
- **Age**: crossed today.
- **Effort**: small (mechanical, low-risk)

#### TD1-084: setup_cornell_scene grew to 296 LOC
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `byroredux/src/cornell.rs` (`setup_cornell_scene`, 244→296 LOC)
- **Status**: NEW
- **Description**: Grew via #2248/#2249's real fog-volume and fire-refraction
  probe setup (confirmed genuine coverage, not stub — see Dimension 6). Test
  harness code, not load-bearing production path.
- **Suggested Fix**: Low priority; extract per-probe setup (fog volume,
  fire-refraction material) into helpers if it grows further.
- **Age**: this window.
- **Effort**: trivial, deferrable

#### TD2-115: Bitangent-sign clamp idiom duplicated across 4 sites, 2 files
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `crates/nif/src/types.rs:161-165` (`bitangent_sign`) vs.
  `crates/nif/src/import/mesh/bs_geometry.rs:188` (new in #2246); plus 2 more
  inline repeats in `bs_geometry_tangent_tests.rs`.
- **Status**: NEW
- **Description**: `bitangent_sign`'s tail idiom (`if dot < 0.0 { -1.0 } else
  { 1.0 }`) is reproduced verbatim by #2246's fix (`if xyzw[3] < 0.0 { -1.0 }
  else { 1.0 }`) — same invariant (clamp a signed value to exactly ±1), two
  different inputs, no shared symbol. The commit message for #2246 explicitly
  frames this as matching `bitangent_sign`'s output convention without
  sharing a function. The test file repeats the same ternary two more times
  as an inline "simulation" per its own comments, rather than calling
  production code.
- **Impact**: No correctness bug today (all sites agree). Risk is future
  convention drift (e.g. a change to zero-tie-break behavior, or a migration
  to `f32::signum` which disagrees on `-0.0`) requiring a grep across 4 sites
  instead of 1 — the same divergent-fix-history failure mode the project's
  already-closed TD2-001 addressed in this same subsystem.
- **Related**: TD2-001 (closed) — same failure class, different call sites.
- **Suggested Fix**: Add a small shared `clamp_sign(x: f32) -> f32` helper
  beside `bitangent_sign` in `crates/nif/src/types.rs`; call it from both
  production sites. Low urgency — reasonable to batch with the next tangent-
  path touch.
- **Age**: `bs_geometry.rs` site is 1 day old (today's #2246); `bitangent_sign`
  predates this window.
- **Effort**: small

#### TD3-206: shader-pipeline.md's volumetrics descriptor-set description is a stale 2026-05-era snapshot
- **Severity**: LOW
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/engine/shader-pipeline.md:357-359`
- **Status**: NEW
- **Description**: The doc describes the volumetrics private descriptor set
  as "froxel image, VolumetricsParams/IntegrationParams UBO, TLAS." The live
  `volumetrics_inject.comp` has 12 bindings (0-11), including `GpuFogVolume`'s
  3 SSBOs (the struct that just received its lockstep test this window, via
  `3f87a865`/#2228/#2231), light/cluster buffers, and density-noise samplers
  — none of which are documented. No code-level drift (the lockstep tests
  are green); this is a pure documentation-completeness gap, not a
  correctness issue.
- **Evidence**: `docs/engine/shader-pipeline.md:357-359` vs. the live
  `layout(set = ..., binding = 0..11)` declarations in `volumetrics_inject.comp`.
- **Suggested Fix**: Expand the binding-list prose to enumerate all 12
  current bindings, or replace with a reference to the GLSL source as the
  single source of truth for binding indices.
- **Age**: dates to the original 2026-05 volumetrics landing; widened by this
  window's #2228/#2231 addition without a doc update.
- **Effort**: small

## Recurring Pattern: Feature Docs Lag Feature Code (4th Consecutive Cycle)

- **07-16 → 07-25**: `feature-matrix.md`'s AI-behavior rows lagged Follow/
  Escort/Guard/Patrol work.
- **07-25 → 08-02**: FSR 3.1 shipped as default upscaler; `shader-pipeline.md`,
  `feature-matrix.md`, `_audit-common.md` all lagged.
- **08-02**: Session 62's shadow-policy/volumetric-fog work outpaced
  `shader-pipeline.md`, `feature-matrix.md`, `ROADMAP.md`.
- **08-03 (this cycle)**: the *fix* to 08-02's `feature-matrix.md` gap
  (TD3-202) itself went stale within ~6 hours as #2236/#2237 closed same-day
  (TD3-205); `renderer.md`'s GPU-size table missed the same `#2219` update
  every sibling doc caught (TD3-204).

Four cycles running, the pattern has shifted slightly: this time it's not
just that docs lag *shipped* code, but that a same-day doc fix can lag a
same-day *issue-closure* — the renderer/NIF feature cadence in this repo is
now fast enough that even audit-driven doc fixes have a sub-day half-life.
Not filed as its own numbered finding (it's a pattern observation across
TD3-204/205, not a fourth instance) but worth naming again given it's now a
4-cycle streak.

## Deferred

None. Every finding in this report is actionable now; no in-progress
milestone gates any of them.
