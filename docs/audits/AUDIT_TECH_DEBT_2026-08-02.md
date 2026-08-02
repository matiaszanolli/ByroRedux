# ByroRedux Tech-Debt Audit — 2026-08-02

Comprehensive-suite leg. Depth: deep (per-instance triage with concrete fix
proposals across all 9 dimensions). Prior report:
`docs/audits/AUDIT_TECH_DEBT_2026-07-25.md`.

## Session Context

92 commits landed between the prior report and this one
(`2cb86be5`..`e4b8b1a1` for most dimensions; test-hygiene diffed
`1a000e35`..`HEAD`, a 71-commit slice of the same window). By far the
dominant feature arc is **Session 62** (2026-07-26 → 2026-08-01): procedural
volumetric fog + clustered local fog volumes, a new
`MATERIAL_KIND_FIRE_REFRACTION` material kind, POM, a shared secondary-ray
tangent-frame include, and a structural shadow-handling refactor (commit
`1fb79038`, "Refactor shadow handling and introduce shared shadow policies")
that introduced two new shader includes (`shadow_common.glsl`,
`shadow_transport.glsl`). Alongside that: a new `crates/hkx` workspace member
(minimal Havok packfile reader for the M47.2 MQ101 cinematic slice), a
resumable NPC-spawn job API (`9bf4c493`), image-space-modifier / cinematic
presentation effects (`4598bc74`), and door-spawn-position selection logic.
The same-day full 23-dimension `docs/audits/AUDIT_RENDERER_2026-08-02.md`
audited Session 62 for *correctness*; this report covers the *debt* around
the same window — dead code left behind, docs that didn't keep pace, and
complexity that regrew around already-fixed items.

All four of the prior report's MEDIUM doc-rot fixes (Vertex byte-size,
`presentation.frag` doc coverage, FSR feature-matrix row, `fsr3-sys` crate
roster) and its one open LOW-duplication item (`frame_upscaler.rs`'s
hand-rolled barrier shape, fixed via #2200 with a pinning regression test)
were verified **landed and holding** — see "Verified Fixed From Prior Cycle"
below. This report focuses on debt introduced or re-grown since 07-25.

## Executive Summary

**19 findings** (2 are "Existing, re-verified — still open / re-grown" status
updates on already-tracked items, not fresh IDs). **0 CRITICAL, 0 HIGH,
3 MEDIUM, 16 LOW.**

Dimension 6 (Stub Implementations) is again almost entirely clean —
`unimplemented!()`/`todo!()`/`panic!("not …")` still **0** repo-wide,
including across three young crates (`pex`, `save`, `scripting`) swept for
the first time this cycle. Dimension 5 (Stale Markers) is effectively clean
— the 17→19 marker-count delta is two same-class false-positive protocol
references, zero genuine new debt. Dimension 9 (Test Hygiene) is effectively
clean — the `#[ignore]`-test streak (zero new since 07-16) finally broke
by exactly one, and it's a legitimate real-corpus opt-in gate.

**All 3 MEDIUM findings are Dimension 3 (doc rot)**, and all three trace to
the same root cause the prior two audits already diagnosed once each: a
large, fast-moving feature arc (this time Session 62's shadow-policy +
volumetric-fog work) outpacing the docs that describe it. This is now the
**third consecutive cycle** this exact failure mode has recurred for a
different feature area (07-16: AI-behavior work; 07-25: FSR 3.1; 08-02:
shadow-policy + volumetrics) — worth naming explicitly as a process gap, not
just three unrelated doc fixes (see "Recurring Pattern" below).

The 16 LOW findings split roughly into three groups:
- **Genuine new dead code** (Dimension 8, 3 findings) — unlike the 07-25
  cycle's `allow(dead_code)` delta (entirely test-exercised scaffolding),
  this cycle's +3 delta is real orphaned code: two unreachable NPC-spawn
  compatibility wrappers superseded by the same commit's own new resumable
  job API, and one untested, uncalled accessor in the brand-new `hkx` crate.
  Plus one pre-existing, previously-missed case (dead NIF particle-modifier
  shims, invisible to the `allow(dead_code)` grep because they're `pub fn`).
- **Complexity re-growth around already-"fixed" items** (Dimension 1,
  4 new + 2 status updates) — `draw_frame`'s specific suggested fix from
  07-25 *did* land (a `build_fsr_frame_parameters` extraction, confirmed
  present with its own tests) but the parent function still net-grew from
  new dispatch code; `VulkanContext::new()` (#1749) grew another 63 LOC with
  no phase extraction started; two files newly crossed 2000 LOC
  (`volumetrics.rs` +903 LOC from the Session-62 feature push,
  `material.rs` +84 LOC from test accumulation); and one function-level
  finding from three months ago (`build_tlas`, TD9-012 in the 05-13 report)
  resurfaces because the file-level split that closed its *container*
  never addressed the function itself.
- **Audit-tooling self-hygiene** (Dimensions 4 and 5, 3 findings) — a new
  crate (`hkx`) missing from `_audit-common.md`'s roster (the same recurring
  gap class as `fsr3-sys` last cycle), a stale file-list in the `XXXX`
  false-positive exclusion, and — the most interesting finding of this
  cycle — **this very skill's own Phase-1 `#[ignore]`-count baseline recipe
  is imprecise**: it scans the whole repo with no `.rs` file-type filter,
  so it picks up every prose mention of "`#[ignore]`" in markdown docs
  (including this skill file and every prior report that quotes the metric)
  and would report "323" today — a false 2.4x "regression" against the
  correctly-scoped "135" the 07-25 report actually measured. See TD4-002.

## Recurring Pattern: Feature Docs Lag Feature Code (3rd Consecutive Cycle)

- **07-16 → 07-25**: `docs/feature-matrix.md`'s AI-behavior rows lagged the
  Follow/Escort/Guard/Patrol runtime work (TD3-101, fixed).
- **07-25 → 08-02** (prior cycle's own findings): FSR 3.1 became the shipped
  default upscaler; `shader-pipeline.md`, `feature-matrix.md`, and
  `_audit-common.md`'s crate roster all lagged (TD3-NEW-01/02/03/04, fixed).
- **08-02 (this cycle)**: Session 62's shadow-policy refactor and volumetric
  fog work outpaced `shader-pipeline.md` (`GpuLight`'s byte-layout table),
  `feature-matrix.md` (Volumetrics row, missing fire-refraction row), and
  `ROADMAP.md` (Tier 8 header self-contradicting the document's own opening
  summary) — TD3-201/202/203.

Each individual fix is trivial-to-small effort and gets applied promptly
once found, but the *pattern* — large fast-moving renderer feature arcs
consistently outpacing the 2-3 docs that are supposed to describe the GPU
contract and shipped-feature status — has now repeated three cycles running
against three different features. Worth considering as a standing process
note (e.g., "renderer feature commits touching `GpuLight`/`GpuMaterial`/
material-kind tables should include the corresponding `shader-pipeline.md`
row in the same commit") rather than continuing to catch it after the fact
each cycle. Not filed as its own numbered finding since it's a pattern
observation across three already-itemized findings, not a fourth instance.

## Baseline Snapshot (for the next audit to diff)

```
TODO/FIXME/HACK/XXX:    19   (was 17; +2, both same-class XXXX-protocol doc references from
                              the closed #1849 fix — zero genuine new markers)
allow(dead_code):       39   (was 36; +3, genuinely new this cycle — 2 orphaned npc_spawn.rs
                              wrappers + 1 unused hkx accessor, unlike the 07-25 delta which
                              was all test-exercised scaffolding)
unimplemented!/todo!(): 0    (unchanged)
#[ignore] tests:        135  (unchanged, scoped to *.rs — the skill's own literal Phase-1
                              recipe returns 323 unscoped; see TD4-002. True attribute-only
                              count, including `#[ignore = "reason"]` forms: 119)
files >2000 LOC:        4    (same count as 07-25, different set — npc_spawn.rs and
                              crates/nif/src/anim/tests.rs dropped below threshold via
                              submodule splits; volumetrics.rs and material.rs newly crossed)
path gate:              GREEN (unchanged — 1116 refs / 26 skill files, 0 stale)
```

Oversized set (live, today):
```
3917  crates/renderer/src/vulkan/context/mod.rs         (Existing: #1749, OPEN — grew 3723→3917; new() 1165→1228 LOC)
3798  crates/renderer/src/vulkan/context/draw.rs         (Existing: TD1-NEW-02, no GH issue filed — grew 3210→3798;
                                                            draw_frame's suggested fix landed but function still 2048→2131 LOC)
2075  crates/renderer/src/vulkan/volumetrics.rs          (TD1-078, NEW — grew 1172→2075 via the Session-62 fog/shadow feature push)
2015  crates/renderer/src/vulkan/material.rs             (TD1-079, NEW — grew 1931→2015, ~all inline test accumulation)
```

Dropped below threshold since 07-25 (both via already-suggested splits,
confirmed applied): `byroredux/src/npc_spawn.rs` (2777→888 LOC, split into
`npc_spawn/{ai_package.rs, resumable.rs, tests.rs}`); `crates/nif/src/anim/tests.rs`
(2002 LOC → split into `anim/tests/{mod,channel,coord_keys,transform,bspline,sanitize,sequence}.rs`,
largest sibling 766 LOC).

## Verified Fixed From Prior Cycle (not re-reported)

- **TD3-NEW-01** (Vertex byte-size): `CLAUDE.md:135` corrected to "104 B ... `cd2b5fe4`". CONFIRMED.
- **TD3-NEW-02** (`presentation.frag` missing): `shader-pipeline.md` now has the row + submission-order step. CONFIRMED.
- **TD3-NEW-03** (FSR missing from feature-matrix.md): Upscaling row added. CONFIRMED.
- **TD3-NEW-04** (`fsr3-sys` missing from crate roster): `_audit-common.md` now lists it, "Crate count: 22". CONFIRMED (see TD4-001 below — a *third* crate has since landed and is itself now missing).
- **TD2-NEW-01** (`frame_upscaler.rs` hand-rolled barrier): fixed via #2200 — now routes through a shared `fsr_input_read_barrier` helper with a dedicated regression test pinning the 4-call shape. CONFIRMED.
- **TD1-NEW-03** (`npc_spawn.rs` file-level crossing): resolved via proper submodule split. CONFIRMED.
- **TD1-NEW-04** (`crates/nif/src/anim/tests.rs` crossing): resolved via exactly the suggested per-phase split. CONFIRMED.

## Top 10 Quick Wins

1. **TD8-001** (15 min) — delete the two orphaned `npc_spawn.rs` compatibility wrappers (`spawn_npc_entity`/`spawn_prebaked_npc_entity`) and their ~6 dangling doc-comment cross-references; nothing calls them.
2. **TD8-003** (10 min) — delete the two dead NIF particle-modifier back-compat shims (`parse_color_modifier`/`parse_simple_color_modifier`); their own doc comments' premise ("few internal call sites") is no longer true.
3. **TD8-002** (5-20 min) — delete `crates/hkx`'s unused `global_target` accessor, or add the one-line "next object type needs this" justification + a test to promote it to legitimate scaffolding.
4. **TD3-201** (trivial) — fix `shader-pipeline.md`'s `GpuLight` byte-table: offsets 52/56 are no longer reserved, they carry the shadow-segment radius and `SHADOW_POLICY_*` encoding since yesterday's `1fb79038`.
5. **TD3-203** (trivial) — resolve `ROADMAP.md`'s self-contradiction: the Tier 8 header still says "No active work" 500 lines after the document's own opening paragraph describes Session 62's shipped volumetric-fog work; annotate M55's row like M59's.
6. **TD3-202** (small) — update `feature-matrix.md`'s Volumetrics row (no longer "content-driven density not wired" — it's wired) and add a `MATERIAL_KIND_FIRE_REFRACTION` row.
7. **TD4-001** (trivial) — add `crates/hkx` to `_audit-common.md`'s crate roster and bump the count 22→23.
8. **TD4-002** (trivial) — rescope the audit skill's own Phase-1/Dimension-9 `#[ignore]`-count greps to `--include='*.rs'`, matching the other two baseline metrics in the same block, so it stops reporting a false 323 vs. the real ~135.
9. **TD5-001** (trivial) — extend the `XXXX`-protocol false-positive exclusion note to cover the two new legitimate reference sites (`esm/cell/wrld.rs`, `esm/cell/mod.rs`) added by #1849's fix.
10. **TD6-001** (trivial) — `ROADMAP.md:757` still calls PACK/QUST/DIAL/MESG/PERK/SPEL/MGEF "stubs (#446/#447)" — all seven have been fully implemented (and the same document says so, 250+ lines later) for months.

*(Also small-effort, not in the top 10 by impact but worth doing opportunistically: TD7-001 — consolidate the 8-layer ray-walk budget hand-declared three times across `raytrace.glsl`/`water.frag`/`shadow_transport.glsl` into one `shader_constants_data.rs` constant; TD2-101 — extract a generic `sample_keyed` helper collapsing `cinematic.rs`'s `sample_scalar`/`sample_color` duplication; TD1-079 — split `material.rs`'s test module into a sibling file, mirroring the `texture_registry.rs`/`texture_registry_tests.rs` precedent already established two files over.)*

## Top 5 Medium Investments

1. **Existing #1749** — `VulkanContext::new()` (1228 LOC, OPEN since 06-29,
   grown 63 LOC further this cycle). Only phase 1 of the issue's own
   4-phase plan (`build_core_device`) has landed. Large effort, GPU-observable
   verification only (not `cargo test`-checkable) — the single largest
   standing complexity item in the codebase.
2. **`draw_frame` re-growth (Existing, no GH issue)** — the 07-25 report's
   specific suggested extraction landed and holds, but the parent function
   re-grew anyway from new shadow-policy/volumetrics dispatch code (+18%
   file-level growth this cycle alone). This is the second time this
   function has been the subject of an extraction that held only days to
   weeks; worth opening a standing tracking issue (trivial) so growth is
   visible between cycles instead of independently rediscovered each time,
   with the actual next extraction (a `build_volumetrics_write`-style helper)
   scoped as small/medium.
3. **TD1-078** — split `volumetrics.rs` (2075 LOC, +903 this cycle) along
   its own construct-vs-record boundary, mirroring the
   `context/{mod,draw,resize,...}.rs` pattern already established for
   `VulkanContext`: pull `new_inner`/`create_volume`/`initialize_layouts`
   into a `volumetrics/init.rs` sibling, leave `dispatch`/`write_*` (already
   unit-tested) in `volumetrics.rs` proper.
4. **TD1-080** — decompose `record_post_passes` (556 LOC, one function
   covering 8+ distinct GPU passes with zero test coverage) into one
   `record_<pass>_pass` helper per pass, preserving call order exactly (no
   barrier/ordering changes — GPU-observable verification only, per this
   repo's standing Vulkan-change caution).
5. **TD1-081** — decompose `build_tlas` (835 LOC, effectively the entire
   `tlas.rs` file). First flagged 3 months ago (05-13, as TD9-012 at 684
   LOC) and never fixed at the function level — the file-level split that
   created `tlas.rs` coincidentally removed it from the discovery command's
   radar without addressing the underlying complaint. Mirror the
   `predicates.rs`-extraction style already used by `blas_static.rs`/
   `blas_skinned.rs` in the same module.

## Findings

### MEDIUM

#### TD3-201: shader-pipeline.md's GpuLight byte-layout table is stale — offsets 52/56 now carry the shadow-segment radius and SHADOW_POLICY_* encoding, not "(reserved)"
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/engine/shader-pipeline.md:288-302` (`GpuLight` — 64 bytes section)
- **Status**: NEW
- **Description**: Commit `1fb79038` ("Refactor shadow handling and introduce shared shadow policies", 2026-08-01) repurposed `GpuLight.params`'s trailing two floats: offset 52 (`params.y`) is now "finite luminous-source radius used by shadow segments," offset 56 (`params.z`) is now `SHADOW_POLICY_*` encoded as f32. Both are consumed by the new `shadow_common.glsl`/`shadow_transport.glsl` includes and by `triangle.frag`/`water.frag`/`volumetrics_inject.comp`. The struct's own Rust doc comment (`gpu_types.rs:183-208`) was updated in the same commit and is accurate; `shader-pipeline.md`'s markdown table — last touched by an unrelated commit three days earlier — still reads "52-63 (reserved)".
- **Evidence**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:203-207` (`/// x = attenuation exponent; y = finite luminous-source radius used by shadow segments; z = SHADOW_POLICY_* encoded as f32; w = reserved.`) vs. `docs/engine/shader-pipeline.md:301-302` ("48 | falloff_exponent | ...", "52-63 | (reserved) | —").
- **Impact**: `shader-pipeline.md` is the project's designated authoritative GPU-layout reference. Notably, the very next day's full 23-dimension renderer audit reviewed this same commit's other changes but didn't catch this specific doc gap — genuinely fresh, unflagged rot.
- **Related**: Same doc, same failure class as the already-fixed TD3-NEW-01 (Vertex byte size).
- **Suggested Fix**: Update to `52 | shadow_segment_radius | Finite luminous-source radius used by shadow segments |`, `56 | shadow_policy | SHADOW_POLICY_* encoded as f32 (see shadow_common.glsl) |`, `60-63 | (reserved) | —`.
- **Age**: 1 day.
- **Effort**: trivial

#### TD3-202: feature-matrix.md's Volumetrics row still says "content-driven density not wired" and omits MATERIAL_KIND_FIRE_REFRACTION entirely
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/feature-matrix.md:47` (Rendering table, Volumetrics row)
- **Status**: NEW
- **Description**: The row reads "~ Scaffold | Froxel injection + integration shaders shipped; content-driven density not wired." Session 62 wired content-driven density (extinction/chromaticity/peak radiance/coverage from CELL/WTHR) — independently confirmed the same day by `AUDIT_RENDERER_2026-08-02.md`, which recommends closing #2220 on that basis. Separately, `MATERIAL_KIND_FIRE_REFRACTION` (103) shipped in the same window with no row anywhere in the table. Third recurrence of the same feature-matrix-lags-shipped-code pattern (after TD3-101, TD3-NEW-03).
- **Evidence**: `docs/feature-matrix.md:47`; `AUDIT_RENDERER_2026-08-02.md`'s "Confirmed fixed" section; `crates/renderer/src/vulkan/scene_buffer/constants.rs:336`.
- **Impact**: A reader would conclude volumetrics is an unfinished scaffold and there's no fire-refraction path at all — both wrong as of this session.
- **Related**: TD3-101 (closed), TD3-NEW-03 (closed) — same file, third recurrence.
- **Suggested Fix**: Change the row to reflect fog + local volumes shipped (partial), add a `MATERIAL_KIND_FIRE_REFRACTION` row noting its known consistency gaps (tracked #2224/#2236/#2237).
- **Age**: 1-7 days.
- **Effort**: small

#### TD3-203: ROADMAP.md's Tier 8 header ("No active work") and M55's row directly contradict the document's own top-of-file Session 62 summary
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `ROADMAP.md:515` (Tier 8 header), `ROADMAP.md:527` (M55 row)
- **Status**: NEW
- **Description**: `ROADMAP.md`'s own opening "Current state" paragraph correctly says Session 62 "shipped ... the renderer's biggest single-session feature push since FSR (procedural volumetric fog, clustered local fog volumes, ...)". 480 lines later, Tier 8's header still reads "No active work — Tier 1-4 ships first" and M55's row ("Volumetric lighting") describes pure future-tense scope with no shipped annotation — unlike M59's row two lines below, which does carry an inline "POM slice shipped 2026-07-29" update.
- **Evidence**: `ROADMAP.md:32-38` vs. `:515` and `:527`; contrast with `:528` (M59's inline update in the same table).
- **Impact**: Self-contradiction inside the project's own stated single source of truth for milestone status — the correct information exists 500 lines away and simply wasn't propagated to the table it's supposed to feed.
- **Related**: None directly.
- **Suggested Fix**: Annotate M55's row mirroring M59's pattern ("Volumetric fog slice shipped 2026-07-26→08-01 (Session 62): ... REGN-driven per-cell height fog and god-ray light-shaft integration remain open") and soften the Tier 8 header.
- **Age**: 1 day.
- **Effort**: trivial

### LOW

#### Existing: #1749 — VulkanContext::new() still a giant constructor, grown further
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1672-2899` (`new()`, ~1228 LOC)
- **Status**: Existing: #1749, OPEN, grown further (1165→1228 LOC; file 3723→3917)
- **Description**: Only the single `build_core_device` extraction (init phase 1) has landed since 06-29; the three deferred phases (`build_swapchain_and_passes`/`build_pipelines`/`build_optional_passes`) are unstarted. This window's growth is mostly shadow-policy/global-only-mesh BLAS plumbing appended inline rather than to a phase helper.
- **Suggested Fix**: Unchanged from the issue's own plan — extract the three deferred phases, verified via `cargo run` + validation layers (GPU-observable only).
- **Age**: constructor complexity dates to 05-13 (TD9-010, 745 LOC); tracked as #1749 since 06-29.
- **Effort**: large

#### Existing: TD1-NEW-02 — draw_frame's suggested extraction landed, but the function re-grew around new dispatch code
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:872-3001` (`draw_frame`, ~2131 LOC)
- **Status**: Existing: TD1-NEW-02 (no GH issue was ever filed for this report-only finding), partially fixed, then re-grown
- **Description**: The 07-25 suggested fix — extract `build_fsr_frame_parameters` — landed exactly as specified (confirmed at `draw.rs:438`, with its own unit tests). `draw_frame` itself still grew 2048→2131 LOC (file 3210→3798, +18%) from the shadow-policy refactor and volumetric/local-fog-volume integration landing more inline dispatch/barrier code.
- **Suggested Fix**: Same pattern again — extract a `build_volumetrics_write(...)` helper and fold the shadow-policy setup near the acceleration-manager calls it feeds. Given two extraction rounds have each held only days to weeks, open a standing tracking issue rather than a recurring report-only finding.
- **Age**: growth accumulated over the 8-day window.
- **Effort**: small (per-block extraction) / trivial (tracking-issue creation)

#### TD1-078: volumetrics.rs crossed 2000 LOC — Session 62 fog/shadow-policy feature push, plus a 556-LOC constructor
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs` (2075 LOC); `VolumetricsPipeline::new_inner` (~556 LOC, line 522)
- **Status**: NEW
- **Description**: Grew 1172→2075 LOC (+77%) driven by procedural fog density/extinction, boot-generated tileable density volumes, clustered local fog volumes, material-aware fog chromaticity, and the shadow-policy refactor's froxel-grid shadow integration. `new_inner` (556 LOC) is the same "every new GPU resource appends another inline block to one giant constructor" shape already tracked for `VulkanContext::new()` (#1749), recurring in a different pipeline object.
- **Suggested Fix**: Split along the file's own phase boundaries — pull `new_inner`'s per-image/per-buffer allocation blocks into named helpers, or move `new_inner`+`create_volume`+`initialize_layouts` into a `volumetrics/init.rs` sibling, keeping the per-frame recording path (`dispatch`/`write_*`) in `volumetrics.rs` proper.
- **Age**: growth accumulated 2026-07-26→08-01 (Session 62).
- **Effort**: medium (GPU-resource half has no easy `cargo test` signal; `dispatch`/`write_*` are already unit-tested and lower-risk)

#### TD1-079: material.rs crossed 2000 LOC — mostly inline test growth
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/renderer/src/vulkan/material.rs` (2015 LOC)
- **Status**: NEW
- **Description**: Grew 1931→2015 LOC (+84), ~60% of the file is `#[cfg(test)]` content. No single production function is anywhere near 200 LOC. Unlike the file's own directory sibling `texture_registry.rs`/`texture_registry_tests.rs`, the tests here were never split out to begin with.
- **Suggested Fix**: Extract the `#[cfg(test)] mod tests` block into a sibling `material_tests.rs`, mirroring `texture_registry.rs`'s precedent. Purely mechanical, lowest-risk finding in this report.
- **Age**: crossed sometime in the 07-25→08-02 window.
- **Effort**: trivial, deferrable

#### TD1-080: record_post_passes is a 556-LOC single function covering 8+ distinct GPU passes inline, zero test coverage
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:137-693`
- **Status**: NEW
- **Description**: `post_passes.rs` was split out of `context/mod.rs`/`draw.rs` under #1857 specifically to isolate post-geometry pass recording — that file-level split succeeded, but `record_post_passes` itself was never further decomposed. It inline-records the water-caustic barrier, SVGF temporal+spatial passes, SSAO, bloom pyramid, volumetrics inject/integrate (now also carrying the shadow-policy-aware TLAS shadow ray), composite, TAA, FSR upscale, and presentation passes back-to-back. No `#[cfg(test)]` block exists in the file at all.
- **Suggested Fix**: Extract each self-contained pass block into its own `record_<pass>_pass(&mut self, cmd, frame, ...)` helper called in sequence, mirroring the boundary already drawn at the file level between `geometry_pass.rs`/`skinned_blas_refit.rs`/`post_passes.rs`. Call-order-preserving only — no barrier/pass reordering.
- **Related**: Existing #1857 (CLOSED, file-level split) — this is the next-level split it didn't reach.
- **Age**: file created ~07-21 under #1857; function has grown steadily since.
- **Effort**: medium (GPU-observable verification only, per this repo's Vulkan-change convention — no test module exists to check against)

#### TD1-081: build_tlas is an ~835-LOC single function — long-standing debt, resurfaces the 05-13 TD9-012 finding at a higher LOC count
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:46-880`
- **Status**: NEW as a report entry (the underlying complaint was first raised 2026-05-13 as TD9-012 at 684 LOC, in the pre-split `acceleration.rs` monolith; no GitHub issue was ever filed and it lapsed once the containing file was split below the 2000-LOC discovery threshold)
- **Description**: `tlas.rs` is essentially one function. Growth has been slow and steady (not a Session-62 spike): 834 LOC (06-02) → 887 today, +151/+22% over three months with no fix landed. It builds/rebuilds the whole per-frame TLAS inline: instance-buffer sizing/rebuild, scratch-buffer growth/shrink, per-draw instance transform + shadow-mask + custom-index assembly, and BUILD-vs-UPDATE dispatch.
- **Suggested Fix**: Mirror the extraction style already used in `blas_static.rs`/`blas_skinned.rs` (both pull named helpers like `scratch_should_shrink`/`decide_use_update` out to `predicates.rs`, already imported here) — extract the instance-buffer rebuild/resize block and the per-draw instance-assembly loop into private helpers. Given 3 months of unaddressed growth across two split cycles, worth a dedicated tracking issue rather than another report-only mention.
- **Related**: TD9-012 (2026-05-13 tech-debt audit, pre-split numbering) — same function, never re-tracked after its containing file was split.
- **Age**: first observed at 684 LOC (05-13); 835 LOC today.
- **Effort**: medium

#### TD2-101: sample_scalar / sample_color in cinematic.rs duplicate the same keyed-linear-interpolation control flow for two value types
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `crates/scripting/src/cinematic.rs:420-464`
- **Status**: NEW
- **Description**: Commit `4598bc74` (image-space-modifier / cinematic presentation work) added `sample_scalar`/`sample_color` — structurally identical keyed-linear-interpolation functions (same empty/before-first/windows(2)-scan/clamp-lerp/after-last shape), differing only in the interpolated value type (`f32` vs `[f32; 4]`). Both are load-bearing for every IMAD channel (blur, grading, tint/fade).
- **Impact**: Cosmetic today; a future change to the interpolation contract (e.g. adding non-linear segments, fixing the degenerate-key edge case) has to be applied at both sites by hand with nothing enforcing lockstep.
- **Suggested Fix**: Extract a generic `sample_keyed<K, V>` helper, or give the key types a shared `time` accessor plus a `Lerp` trait implemented for `f32` and `[f32; 4]`. Collapses ~45 duplicated lines to ~20.
- **Age**: 1 day.
- **Effort**: small

#### TD4-001: _audit-common.md's "22-crate roster" is stale — crates/hkx is missing, live count is 23
- **Severity**: LOW
- **Dimension**: 4 (Audit-Finding Rot)
- **Location**: `.claude/commands/_audit-common.md:120-126`, `.claude/commands/audit-tech-debt/SKILL.md:21`
- **Status**: NEW
- **Description**: `crates/hkx` (added Session 62 for the MQ101 cinematic slice — minimal Havok packfile reader) is absent from both the enumerated list and the "Crate count: 22" line. `ROADMAP.md` is already correct ("23 crates"). Same root-cause pattern as the already-fixed TD3-NEW-04 (a new crate landing without a matching roster update) — this is a fresh recurrence for a different crate, one cycle later.
- **Suggested Fix**: Add `hkx` to the enumerated list with a short parenthetical (mirroring the `fsr3-sys` treatment); bump the count to 23; update the skill's "22-crate roster" phrase.
- **Age**: `crates/hkx` landed in the Session 62 MQ101 slice.
- **Effort**: trivial

#### TD4-002: This skill's own Phase-1 #[ignore]-count baseline recipe scans the whole repo textually, producing a false ~2.4x "regression" signal
- **Severity**: LOW
- **Dimension**: 4 (Audit-Finding Rot)
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md` (Phase 1 baseline snapshot; Dimension 9 discovery command) — both use `grep -RIn '#\[ignore\]' .` with no `--include='*.rs'` filter
- **Status**: NEW
- **Description**: The recipe scans every tracked file, not just `.rs` sources — it picks up every prose mention of the literal string `#[ignore]` inside markdown, including this very skill file's own Dimension-9 text and every prior `docs/audits/*.md` tech-debt report that quotes an `#[ignore]` count (a self-reinforcing inflation: each report that prints the raw count adds another hit for the next report). Verified: raw repo-wide = 323; scoped to `.rs`, `target/` excluded = 135; actual attribute lines only = 96. The 07-25 report's baseline of "135" was the correctly-scoped figure — but the skill's own committed recipe, run literally, returns 323 today, which reads as a false "+139%" regression.
- **Impact**: Any future run of this audit's own Phase-1 snapshot or Dimension-9 discovery would misdiagnose a large `#[ignore]`-test debt spike that doesn't exist — the entire delta is markdown noise. This is exactly the audit-finding-rot failure mode Dimension 4 exists to catch, except the rot is in the audit tool's own measurement.
- **Suggested Fix**: Scope both greps to `--include='*.rs'` and exclude `target/`, matching the other two Phase-1 metrics in the same block (both already scoped to `crates byroredux`). Recommended: `grep -RIn '^\s*#\[ignore\]' --include='*.rs' crates byroredux | wc -l`.
- **Age**: the recipe has read this way since the skill was authored; only became visible as a live discrepancy once enough prior reports accumulated textual mentions.
- **Effort**: trivial

#### TD5-001: XXXX-protocol exclusion list doesn't yet name the two newest reference sites
- **Severity**: LOW
- **Dimension**: 5 (Stale Markers)
- **Location**: Dimension 5's false-positive exclusion note (currently scoped to `reader.rs`/`magic.rs` only)
- **Status**: NEW
- **Description**: Commit `560c6741d` (closes #1849) added two more legitimate `XXXX`-protocol references at `crates/plugin/src/esm/cell/wrld.rs:175` and `crates/plugin/src/esm/cell/mod.rs:871` — same false-positive class as the existing exclusion, but the exclusion's file list is now stale, forcing every future audit to re-derive "new marker or same protocol tag in a new file" from scratch.
- **Suggested Fix**: Extend the exclusion bullet to include `esm/cell/wrld.rs`/`esm/cell/mod.rs`, or better, key the exclusion on content ("any comment referencing the ESM `XXXX` extended-size escape") rather than enumerating file paths.
- **Age**: 1 week (2026-07-26).
- **Effort**: trivial

#### TD6-001: ROADMAP.md's "Completed Milestones" one-liner still calls PACK/QUST/DIAL/MESG/PERK/SPEL/MGEF "stubs" — all seven fully implemented for months
- **Severity**: LOW
- **Dimension**: 6 (Stub & Placeholder Implementations) / overlaps Dimension 3
- **Location**: `ROADMAP.md:757`
- **Status**: NEW
- **Description**: The one-liner reads "PACK / QUST / DIAL / MESG / PERK / SPEL / MGEF stubs (#446/#447)," dating to 2026-04-22 (the original M24 Phase 1 scope). The same document's own later rows (M24.2, M42, Known Issues) explicitly say these are closed/implemented, and the actual parsers are substantial (1,881/1,283/565/1,351 LOC).
- **Impact**: A reader skimming the "Completed Milestones" fast-reference section would conclude seven heavily-developed parsers (load-bearing for shipped M42.x-M47.x gameplay) are unimplemented.
- **Suggested Fix**: Replace with "now fully parsed (#446/#447 closed; see M24.2/M42 rows for decode detail)."
- **Age**: 102 days.
- **Effort**: trivial

#### TD7-001: MAX_TRANSPARENT_SKIPS / MAX_OPAQUE_LAYERS — the same 8-layer ray-walk budget is hand-declared independently in three GLSL files
- **Severity**: LOW
- **Dimension**: 7 (Magic Numbers & Hardcoded Constants)
- **Location**: `crates/renderer/shaders/include/raytrace.glsl:64`, `water.frag:252`, `include/shadow_transport.glsl:11`
- **Status**: NEW
- **Description**: Three GLSL functions each declare an 8-layer bounded ray-walk cap under two different names (`MAX_TRANSPARENT_SKIPS` twice, `MAX_OPAQUE_LAYERS` once, the latter added today by the shadow-policy refactor) instead of sourcing from `shader_constants_data.rs`, the project's documented single source of truth for values shared between Rust and GLSL. Two of three sites are new since 07-25; the pattern is regrowing rather than converging — Session 62's new module invented yet another name for the same value.
- **Impact**: Cosmetic today (all three are still numerically 8), but a future tuning pass has three independent call sites to update by hand with no tripwire if one is missed.
- **Related**: Distinct from Existing #2229 (REN-D3-02, a Rust-const-vs-GLSL bypass) — this is GLSL-vs-GLSL, no Rust side involved.
- **Suggested Fix**: Add a shared constant to `shader_constants_data.rs`, `#include` at all three sites, replace the local declarations.
- **Age**: raytrace.glsl's copy predates the window; water.frag's is 7 days old; shadow_transport.glsl's is 0 days old (today).
- **Effort**: small

#### TD8-001: Orphaned synchronous NPC-spawn compatibility wrappers
- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `byroredux/src/npc_spawn.rs:716` (`spawn_npc_entity`), `:815` (`spawn_prebaked_npc_entity`)
- **Status**: NEW
- **Description**: Commit `9bf4c493` introduced the resumable `NpcSpawnJob` API and, in the same commit, tagged the two older synchronous entry points `#[allow(dead_code)]`, framing them as "compatibility entry points." No caller exists: the only real spawn site (`cell_loader/references/mod.rs`) drives `NpcSpawnJob` directly. Zero call sites anywhere in the repo outside the two definitions and doc-comment mentions.
- **Impact**: Two ~30-line `pub fn`s with 12 parameters each exist as purely unreachable API surface, including an untested `unreachable!()` branch inside.
- **Suggested Fix**: Delete both functions and their ~6 dangling doc-comment cross-references. If a synchronous unlimited-budget spawn is ever needed again, it's three lines to re-derive from `NpcSpawnJob::runtime(...).advance(..., &mut FrameTimeBudget::unlimited())`.
- **Age**: 6 days.
- **Effort**: 15 minutes

#### TD8-002: Unused global_target fixup accessor in new hkx crate
- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/hkx/src/packfile.rs:185-193`
- **Status**: NEW
- **Description**: `Packfile::global_target` (cross-section fixup resolution) has zero call sites anywhere in the crate — only its sibling `local_target` (same-section fixups) is used. Unlike the 07-25 cycle's `quest.rs` scaffolding precedent, this has no justifying comment and no test coverage at all.
- **Suggested Fix**: Delete until a Havok object type that needs cross-section fixups is decoded (the current skeleton + spline-animation decoders don't), or add a one-line justification comment + placeholder test to promote it to legitimate scaffolding.
- **Age**: 1 day (newest code in the repo).
- **Effort**: 5-20 minutes

#### TD8-003: Dead NIF particle-modifier back-compat shims (pre-existing, missed by the 07-25 sweep)
- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/nif/src/blocks/particle.rs:322` (`parse_color_modifier`), `:607` (`parse_simple_color_modifier`)
- **Status**: NEW (as a reported finding — the code predates 07-25; it carries no `#[allow(dead_code)]` because both are `pub fn`, which suppresses rustc's lint even though nothing calls them, so it was invisible to the standard discovery grep)
- **Description**: Both are documented as "kept so the few internal call sites that only need byte-correct stream advancement still compile" — false today; the block dispatcher calls the typed `parse` methods directly, exactly as the shims' own doc comments recommend new code do.
- **Suggested Fix**: Delete both.
- **Age**: predates the 07-25 baseline by several weeks; missed because `pub fn` items aren't caught by `allow(dead_code)`-based discovery.
- **Effort**: 10 minutes

#### TD9-001: One new #[ignore] added since 07-25 — legitimate real-corpus gate, breaks the "zero new" streak
- **Severity**: LOW
- **Dimension**: 9 (Test Hygiene)
- **Location**: `crates/ui/src/host/tests.rs:350`
- **Status**: NEW
- **Description**: `installed_fallout4_representative_menus_obey_host_object_lifecycle` was added with `#[ignore = "requires an installed Fallout 4 corpus"]` — the only `#[ignore]` addition in the 71-commit window, mirroring an existing Skyrim sibling one line above it. The 07-16→07-25 "zero new #[ignore]" streak (110 commits) doesn't extend through 08-02.
- **Impact**: None — matches the established real-corpus opt-in gating pattern exactly. Filed only so the running delta count stays accurate for the next cycle.
- **Suggested Fix**: None needed.
- **Age**: within this window.
- **Effort**: n/a (tracking only)

## Verified Clean

- **Dimension 5 (Stale Markers)**: 17→19, both new hits same-class false positives (protocol `XXXX` doc references from #1849's fix). Zero genuine new markers across the Session-62 window specifically checked commit-by-commit.
- **Dimension 6 (Stub Implementations)**: `unimplemented!()`/`todo!()`/`panic!("not …")` still 0 repo-wide, including a first-ever sweep of `crates/pex/`, `crates/save/`, `crates/scripting/` (young crates never previously audited for this dimension). Console commands: zero no-op/TODO placeholders. `crates/plugin/src/legacy/` confirmed to hold only the FormId bridge (#390 removal still holds).
- **Dimension 7 (Magic Numbers)**: `GpuCamera`(336B)/`GpuInstance`(112B)/`GpuMaterial`(348B) lockstep unchanged and correctly test-pinned. Vulkan device limits correctly queried at runtime, not hardcoded. NIF version-gate bare literals in `morph.rs` are old, extensively commented, and deliberately not consolidated with a coincidentally-matching but semantically-different constant. Session 62's one NIF commit (`e3b9b115`) minted a new named constant (`SF_WEAK_REF_GAP`) correctly rather than hardcoding — a positive example. The `if data.len() == 24` pattern named in the discovery recipe does not exist anywhere in `crates/plugin/src/esm/` — every size check uses a progressive `>=` gate.
- **Dimension 8 (Dead Code)**: Session 62's shadow-policy refactor (`1fb79038`, 174 GLSL lines relocated) verified as a clean relocation with zero leftovers — old per-shader duplicate occlusion helpers fully removed, `lights.rs`'s sentinel writes replaced with named constants, `cargo check --workspace --all-targets` zero warnings. No new `#[deprecated]`, `// removed:` breadcrumbs, or refactor-leftover `_`-prefixed parameters.
- **Dimension 9 (Test Hygiene)**: 244 tests added / 0 removed this window. The one closed-CRITICAL/HIGH `#[ignore]`-guard trigger found (`#533`/`#819`/`#965` in `parse_real_esm.rs`) is pre-existing, already tracked since 2026-05-14 (TD6-102/104/105), not new. `golden_frames.rs`'s baseline confirmed unaffected by the volumetric-fog rewrite (the demo scene it renders has no active `FogMedium`, traced through the `unwrap_or_default()` fallback). Young-crate assert density (`pex`/`save`/`scripting`, including ~9,374 new Session-62 lines) healthy throughout.
- **Dimension 2 (spot checks beyond TD2-101)**: the shadow-policy decode correctly funnels through `shadow_common.glsl`/`shadow_transport.glsl` across all four consumers with zero inline reimplementation. `volumetrics.rs`'s new fog-volume upload code follows the file's own established per-frame-in-flight buffer pattern (legitimate repetition, not drift). Texture-upload chains, the new IMAD ESM parser, Z-up→Y-up call sites (all ~20, including three canonical homes now confirmed — `crates/nif/src/import/coord.rs`, `crates/nif/src/anim/coord.rs`, and `crates/core/src/math/coord.rs`), and `WriteDescriptorSet` boilerplate all correctly route through their existing consolidation points.
- **Dimension 4 (further)**: no real "Existing: #NNN" callouts needing reframing found across all 23 audit skills (only a fictional worked-example row). 8-symbol spot-check of gate-invisible references all resolved correctly. Sampled the two oldest (~119-day) reports with HIGH/CRITICAL findings (`AUDIT_ECS_2026-04-04.md`, `AUDIT_RENDERER_2026-04-02.md`, `AUDIT_LEGACY_COMPAT_2026-04-05.md`) — all findings confirmed fixed via code archaeology, just never looped back into the report text (expected for immutable snapshots).

## Deferred

None. Every finding in this report is actionable now; no in-progress
milestone gates any of them. #1749 (`VulkanContext::new()`) and the three
Dimension-1 complexity items are large/medium-effort standing backlog
items rather than milestone-blocked — they're listed under "Top 5 Medium
Investments" rather than here because nothing prevents starting them today,
they're simply not trivial.
