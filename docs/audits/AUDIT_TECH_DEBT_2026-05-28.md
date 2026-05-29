# Tech-Debt Audit — 2026-05-28

11-dimension sweep. Prior reports: [2026-05-22](AUDIT_TECH_DEBT_2026-05-22.md), [2026-05-24](AUDIT_TECH_DEBT_2026-05-24.md).

---

## 1. Executive Summary

**26 findings** across 11 dimensions — 0 HIGH, 4 MEDIUM, 19 LOW, 2 INFO, plus 1 MEDIUM and 1 LOW carried
as DEFERRED (gated on open milestones). This is the largest single-sweep new-finding count in the
audit history; the growth is driven by the 4-day gap since 2026-05-24 absorbing the
M58/M55/water-caustic/Disney-BSDF/FO4-SCOL/FO76-gear band (~76 commits on the renderer alone).

The two BLOCKED Vulkan-recording monoliths (TD9-200/TD9-201) have escalated further: `draw.rs` is now
**3337 LOC** (+104 vs 2026-05-24) and `context/mod.rs` is **3017 LOC** (+135). Three binary-crate files
crossed or approached the 2000-LOC ceiling for the first time: `asset_provider.rs` (2561), `commands.rs`
(2115), and `main.rs` (2448, crossing the #1267 promotion threshold). The `parse_esm_with_load_order`
879-LOC monolith is a new entrant in the Medium Investments table.

New dimensions covered for the first time in this cycle: Dim 2 (dead code), Dim 3 (duplication),
Dim 5 (C++ bridge stubs), Dim 6 (assertion-free tests), Dim 8 (API surface / re-export hygiene).

| Severity | NEW | Carryover | Total | Dimensions affected |
|----------|-----|-----------|-------|---------------------|
| HIGH     | 0   | 0         | 0     | — |
| MEDIUM   | 3   | 2         | 5     | D9 (TD9-200/201/NEW-02–05), D5 (TD5-NEW-01) |
| LOW      | 17  | 2         | 19   | D2, D3, D4, D6, D7, D8, D9 |
| INFO     | 2   | 0         | 2    | D4, D6 |
| DEFERRED | 1M+1L | —      | 2    | D5 (TD5-NEW-02), D9 (TD9-NEW-04 partial) |

Delta vs 2026-05-24: **+23 net new findings** (no closures this cycle — no fixing sprint ran between
2026-05-24 and 2026-05-28).

---

## 2. Baseline Snapshot

Source: `/tmp/audit/tech-debt/baseline.txt` (captured 2026-05-28 pre-sweep).

| Metric | 2026-05-28 (today) | 2026-05-24 | 2026-05-22 | Δ (28 vs 24) |
|---|---:|---:|---:|---:|
| `TODO` / `FIXME` / `HACK` / `XXX` markers (raw grep) | 4 | 5 | 4 | **−1** |
| ↳ of which *active* (not closure-mention / prose ref) | **1** | **1** | **0** | 0 |
| `#[allow(dead_code)]` | 27 | 26 | 26 | +1 |
| `#[allow(unused...)]` | — | 21 | 20 | — |
| `unimplemented!()` / `todo!()` | 0 | 0 | 0 | 0 |
| `panic!("not yet"\|"not impl")` | 0 | 0 | 0 | 0 |
| `#[ignore]` tests (raw grep) | 115 | 113 | 126 | +2 |
| ↳ true attribute-position count (see TH6-NEW-04) | **96** | ~95 | ~110 | — |
| Files > 2000 LOC | **5** | 3 | 2 | **+2** |
| `.claude/commands/_audit-validate.sh` | not re-run this cycle | OK (293 refs) | OK | — |

**Files > 2000 LOC (current):**

| File | LOC (2026-05-28) | LOC (2026-05-24) | Δ |
|---|---:|---:|---:|
| `crates/renderer/src/vulkan/context/draw.rs` | **3337** | 3233 | +104 (see note) |
| `crates/renderer/src/vulkan/context/mod.rs` | **3017** | 2882 | +135 |
| `byroredux/src/asset_provider.rs` | **2561** | ~1815 | **+746** NEW crossing |
| `byroredux/src/main.rs` | **2448** | 2162 | **+286** (crosses #1267 threshold) |
| `byroredux/src/commands.rs` | **2115** | ~1600 | **+515** NEW crossing |

> Note on draw.rs delta: the −207 figure reflects a targeted extraction committed 2026-05-26 that moved
> the bloom pass recording to `bloom.rs`. The underlying file is still 3337 LOC and the split precondition
> (RenderDoc capture) has not been met — it remains BLOCKED and MEDIUM.

---

## 3. Top 10 Quick Wins

Trivial-to-small effort, immediate readability or compile-time payoff. No Vulkan recording changes.
Listed in suggested execution order.

1. **D2-NEW-01** — Delete `EguiPassConfig` struct (7 lines, zero callers). `crates/debug-ui/src/lib.rs:208-214`.
2. **TD7-NEW-01** — Update 8 occurrences of `"260 bytes"` → `"300 bytes"` in `material.rs` doc comments and `triangle.frag` GLSL header. Grep-and-replace, 2 files.
3. **TD7-NEW-02** / **TD4-NEW-10** — Update `GpuCamera` doc comments from 288 B → 304 B in `gpu_types.rs` (2 sites) and `context/mod.rs` (1 site). Three-line fix.
4. **TD7-NEW-03** — Replace four phantom `Material::classify_pbr` method references with the live `classify_pbr_keyword` free function name. Four doc-comment edits across 2 files.
5. **TD7-NEW-04** — Update `triangle.frag:83-126` stale line-range citations → `triangle.frag:110-184` in `material.rs` (3 sites).
6. **TD4-NEW-11** — Fix `MAX_MATERIALS` doc comment: `16384 × 304 B = 4.75 MB` → `16384 × 300 B ≈ 4.69 MB`. `constants.rs:150`, one line.
7. **TD4-NEW-12** — Update `flat_shading_bit_pinned_at_128_for_shader_constant` test comment to reflect named-constant usage (the shader no longer uses `128u` directly). `constants.rs:334-343`.
8. **TD3-NEW-A** — Delete `read_zstring` from `cell/helpers.rs` (3 lines) and point its 3 callers at the identical function already in `records/common.rs`.
9. **TD8-D8-NEW-01** — Delete `pub use legacy::{LegacyFormId, LegacyLoadOrder};` from `crates/plugin/src/lib.rs:35` (zero external consumers).
10. **D2-NEW-02** — Remove `class_by_type_id` HashMap from `sfmaterial/src/reader.rs` State struct (field + init + 1 insert, no reads anywhere in codebase).

---

## 4. Top 5 Medium Investments

File / function splits and duplication consolidations; each has a clear split axis and no Vulkan recording
boundary unless noted.

1. **TD9-200 / TD9-201 (BLOCKED, escalating)** — `draw.rs` (3337 LOC) and `context/mod.rs` (3017 LOC)
   are the largest files in the codebase and growing. Fix path: RenderDoc captured-frame baseline
   precondition first; then extract one submodule per render pass. Both files are now above 3000 LOC —
   the "BLOCKED" label does not mean "safe to ignore," it means the unblocking action (RenderDoc harness)
   is the highest-leverage single investment on the board.

2. **TD9-NEW-05** — `parse_esm_with_load_order` is 879 LOC with a 109-arm `match`. Promote the dispatch
   body to a `GrupDispatcher` struct; each GRUP type becomes an independently testable `parse_<type>_grup`
   fn. No Vulkan surfaces, splittable today. `crates/plugin/src/esm/records/mod.rs:119`.

3. **TD9-NEW-02 / TD9-NEW-03** — `asset_provider.rs` (2561 LOC) and `commands.rs` (2115 LOC) both
   crossed the 2000-LOC ceiling this cycle. Neither has Vulkan recording; both split cleanly along
   provider/command-domain axes. See findings for proposed module layouts.

4. **TD5-NEW-01** — M47.1 condition evaluator has 6 stub branches silently returning hardcoded safe-defaults
   for `GetFactionRank`, `HasPerk`, `GetActorValue`, `GetDistance`, `RunOn::Reference`, and Global comparand.
   These corrupt quest-stage gates, dialogue conditions, and AI package activation on every evaluation.
   Wire the missing ECS components or file a single tracking issue.

5. **TD3-NEW-B** — Z-up→Y-up coord-flip leaked into 4 sites outside the canonical
   `byroredux_core::math::coord::zup_to_yup_pos`. Four ≤2-line call-site replacements, but the pattern
   keeps re-appearing; worth treating as a sweep to also add a lint or re-audit policy.

---

## 5. Findings

### HIGH

None.

---

### MEDIUM

#### TD9-200 *(carry, BLOCKED, ESCALATING)* — `draw.rs` 3337-LOC recording monolith

- **ID**: TD9-200
- **Title**: `draw_frame` is a 3337-LOC file (draw_frame fn ~3183 LOC); monolith — carry escalating (+104 LOC since 2026-05-24)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:117`
- **Evidence**: `draw_frame` spans lines 117–3142 = 3026 LOC. File total 3337 LOC. All 9 render
  passes recorded inline as sequential code blocks; pass headers identifiable only via comments
  (G-buffer ~1666, SVGF ~2575, TAA ~2766, SSAO ~2798). Nesting depth peaks at 7 (lines 739–769).
- **Fix**: RenderDoc captured-frame baseline precondition first. Then extract one submodule per pass:
  `gbuffer_pass.rs`, `skin_compute_pass.rs`, `blas_tlas_pass.rs`, `ssao_pass.rs`, `svgf_pass.rs`,
  `taa_pass.rs`, `volumetrics_pass.rs`, `composite_pass.rs`. `draw_frame` becomes a sequenced dispatcher.
- **Effort**: large (blocked on RenderDoc harness)
- **Age**: pre-Session-34 original; last touched commit `9aa69c68` (2026-05-28)

#### TD9-201 *(carry, BLOCKED, ESCALATING)* — `context/mod.rs` 3017-LOC orchestration monolith

- **ID**: TD9-201
- **Title**: `context/mod.rs` at 3017 LOC — carry escalating (+135 LOC since 2026-05-24)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs`
- **Evidence**: 3017 LOC (+135 vs 2026-05-24 baseline of 2882). `VulkanContext::new()` init chain
  plus `Drop` impl (243 LOC, lines ~2565–2808 — teardown of all Vulkan objects). Nesting depth 7
  at lines 1535–1536.
- **Fix**: Same RenderDoc precondition as TD9-200. Split axes: `context/init.rs` (new() chain),
  `context/teardown.rs` (Drop body), `context/state.rs` (subsystem accessors). Struct definition
  stays in `mod.rs`.
- **Effort**: large (blocked on RenderDoc harness)
- **Age**: pre-Session-34 original; last touched commit `06af4698` (2026-05-28)

#### TD5-NEW-01 — M47.1 condition evaluator: 6 stub branches returning hardcoded safe-defaults

- **ID**: TD5-NEW-01
- **Title**: M47.1 condition evaluator: 6 stub branches returning hardcoded safe-defaults (GetFactionRank, HasPerk, GetActorValue, GetDistance, RunOn::Reference, Global comparand)
- **Dimension**: 5 (Stub / Placeholder Implementations)
- **Location**: `crates/scripting/src/condition.rs:300-346` (function stubs); `:170-187` (RunOn::Reference + QuestAlias/PackageData/EventData); `:212-223` (Global comparand)
- **Evidence**: `GetFactionRank` always returns −1.0 ("FactionMembership component not yet plumbed").
  `HasPerk` always returns 0.0 ("PerkList component not yet plumbed"). `GetActorValue` always
  returns 0.0 ("AVIF→ActorStats key resolver deferred"). `GetDistance` always returns 0.0
  ("FormID→EntityId resolver deferred"). `RunOn::Reference` always `None`. Global comparand always
  0.0 ("GLOB lookup deferred"). All silently evaluated on every CTDA predicate — always-false or
  always-correct returns corrupt quest stage gates, AI package conditions, dialogue conditions.
  No GitHub issue filed.
- **Fix**: Wire `FactionMembership`/`PerkList` components (M50 perk system), AVIF→ActorStats lookup
  (issue #519), FormID→EntityId reverse-lookup (`World::find_by_form_id`, #1212 prerequisite), and
  GLOB lookup via `EsmIndex.globals`. File a single tracking issue for the M47.1 follow-up bundle.
- **Effort**: medium (multiple ECS wiring points; coordinate with M50 timeline)
- **Age**: introduced with M47.1 landing; no prior audit coverage

---

### LOW

#### Dimension 2 — Dead Code / Unused Public API

##### D2-NEW-01 — EguiPassConfig declared but never referenced

- **ID**: D2-NEW-01
- **Location**: `crates/debug-ui/src/lib.rs:208-214`
- **Evidence**: `pub struct EguiPassConfig { ... }` — workspace-wide grep for `EguiPassConfig` returns
  exactly one hit: the declaration. The renderer constructs `EguiPass::new()` at
  `context/mod.rs:2533` by passing individual args, bypassing this config bundle entirely.
- **Fix**: Delete the struct (7 lines). Re-introduce if a typed config bundle is needed when
  Phase 5 wires cell-load from the debug-UI.
- **Effort**: trivial

##### D2-NEW-02 — `State::class_by_type_id` HashMap built but never queried

- **ID**: D2-NEW-02
- **Location**: `crates/sfmaterial/src/reader.rs:39, 60, 186`
- **Evidence**: `class_by_type_id: HashMap<u32, usize>` initialized empty, populated via
  `state.class_by_type_id.insert(class.type_id, idx)` for every class, but zero reads anywhere
  in the file or workspace. No `#[allow(dead_code)]` — silenced by private struct scope.
- **Fix**: Remove the field and its two touch-points (init + insert). The public
  `ComponentDatabaseFile` struct already exposes `class_by_name_offset`; add type-id lookup there
  if Phase 2 needs it.
- **Effort**: trivial

##### D2-NEW-03 — sfmaterial / debug-ui export unused public surface

- **ID**: D2-NEW-03
- **Location**: `crates/sfmaterial/src/lib.rs:58-63`; `crates/debug-ui/src/lib.rs:43, 199-200`
- **Evidence**: Workspace-wide grep for `use byroredux_sfmaterial::` finds exactly one import site
  (`asset_provider.rs:7`, imports only `ComponentDatabaseFile`). Eleven other re-exports
  (`ChunkType`, `Error`, `Result`, `StringTable`, `BuiltinType`, `Class`, `ClassFlags`, `Field`,
  `TypeReference`, `ObjectInstance`, `Ref`, `Value`) have zero external consumers. `debug-ui`
  re-exports `PanelTab`, `egui`, `egui_winit` — zero callers outside the crate.
- **Fix**: For sfmaterial: keep `Error`/`Result`/`ComponentDatabaseFile`; mark the other 9 types
  `#[doc(hidden)]` or `pub(crate)` until Phase 2. For debug-ui: add a doc comment clarifying the
  re-exports are convenience surface for future panel-extension authors, or remove them.
- **Effort**: small

#### Dimension 3 — Code Duplication

##### TD3-NEW-A — `cell/helpers.rs::read_zstring` duplicates `records/common.rs::read_zstring` verbatim

- **ID**: TD3-NEW-A
- **Location**: `crates/plugin/src/esm/cell/helpers.rs:7-10` vs `crates/plugin/src/esm/records/common.rs:138-143`
- **Evidence**: Identical function bodies — both use `data.iter().position(|&b| b == 0).unwrap_or(data.len())`.
  `cell/helpers.rs` was created 2026-04-26 (`a231fd50`) during the cell-monolith split and did not
  import from `records/common`. Three consumers (`cell/walkers.rs:5`, `cell/wrld.rs:5`,
  `cell/support.rs:5`) use `super::helpers::read_zstring`.
- **Fix**: Delete `read_zstring` from `cell/helpers.rs`. Add
  `pub(super) use crate::esm::records::common::read_zstring;` in its place (or update the three
  call-site imports). `read_form_id` and `read_form_id_array` in `cell/helpers.rs` are NOT
  duplicates (different signatures) and should stay.
- **Effort**: trivial
- **Age**: `a231fd50` (2026-04-26)

##### TD3-NEW-B — Z-up→Y-up coord-flip leaked into 4 sites outside canonical helper

- **ID**: TD3-NEW-B
- **Location**: `byroredux/src/systems/particle.rs:54-55`, `crates/nif/src/import/collision.rs:527-529`,
  `byroredux/src/cell_loader/references.rs:1058`, `byroredux/src/cell_loader/transition.rs:135-136`
- **Evidence**: All four implement the canonical `(x, z, -y)` transform already in
  `byroredux_core::math::coord::zup_to_yup_pos`. Sites:
  1. `particle.rs:54-55` — local closure `fn zup_to_yup(v) { [v[0], v[2], -v[1]] }`, commit `f31de708` (2026-05-13)
  2. `collision.rs:527-529` — `fn havok_to_engine(x,y,z) { Vec3::new(x,z,-y) }`, commit `dfb26ae3` (2026-04-05), predates #1044 consolidation (2026-05-14), not migrated
  3. `references.rs:1058` — inline `[t.x, t.z, -t.y]` return, commit `3347a930` (2026-05-24)
  4. `transition.rs:135-136` — `pub fn position_zup_to_yup(p) { Vec3::new(p[0],p[2],-p[1]) }`, not migrated by #1044
- **Fix**: Four trivial ≤2-line replacements using
  `byroredux_core::math::coord::zup_to_yup_pos` (returns `[f32;3]`; wrap in `Vec3::from()` for
  Vec3 return sites). All four files already depend on `byroredux_core`.
- **Effort**: trivial
- **Age**: oldest site `dfb26ae3` (2026-04-05); newest `3347a930` (2026-05-24)

##### TD3-NEW-C — `water.rs` bypasses `DescriptorPoolBuilder` with raw `create_descriptor_pool`

- **ID**: TD3-NEW-C
- **Location**: `crates/renderer/src/vulkan/water.rs:242-255`
- **Evidence**: Hand-rolled `[DescriptorPoolSize::default().ty(STORAGE_IMAGE)...]` +
  `DescriptorPoolCreateInfo` + `create_descriptor_pool` (introduced `5f1a9158`, 2026-05-24).
  Every other descriptor pool in the renderer uses `DescriptorPoolBuilder`. The builder landed
  earlier (#1030).
- **Fix**: Replace the 10-line raw creation with the `DescriptorPoolBuilder::new().pool(...).max_sets(...).build(...)` chain. Add `use super::descriptors::DescriptorPoolBuilder;` (already imported in sibling files).
- **Effort**: trivial
- **Age**: `5f1a9158` (2026-05-24)

#### Dimension 4 — Magic Numbers / Stale Constants / Doc Drift

##### TD4-NEW-10 — GpuCamera doc comment says 288 B; struct is 304 B since #1210

- **ID**: TD4-NEW-10
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:171,175`; `crates/renderer/src/vulkan/context/mod.rs:677`
- **Evidence**: `gpu_types.rs:171`: `"GPU-side camera data (288 bytes, std140-compatible)."` Commit
  `8a1a06b4` (2026-05-24) added `pub sun_direction: [f32; 4]` (+16 B, 7th vec4), updated the test
  assertion to 304, but left the doc comment at 288. `context/mod.rs:677`: `"doesn't touch GpuCamera's 288 B layout"` is also stale. Test `gpu_camera_is_288_bytes` correctly asserts 304 B.
- **Fix**: `gpu_types.rs:171` → `"304 bytes"`; `:175` → `"seven trailing vec4 (7×16 = 112 B: …, sun_direction) → 304 B"`. `context/mod.rs:677` → `"304 B"`.
- **Effort**: trivial
- **Age**: `8a1a06b4` (2026-05-24)

##### TD4-NEW-11 — MAX_MATERIALS doc comment cites 304 B for GpuMaterial; actual size is 300 B

- **ID**: TD4-NEW-11
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:150`
- **Evidence**: Line 150: `"MAX_MATERIALS: 16384 × 304 B = 4.75 MB"`. The live pin
  `assert_eq!(std::mem::size_of::<GpuMaterial>(), 300)` in `material.rs:1157` is authoritative.
  Correct computation: 16384 × 300 B = 4.69 MB.
- **Fix**: Change `16384 × 304 B = 4.75 MB` → `16384 × 300 B ≈ 4.69 MB`.
- **Effort**: trivial
- **Age**: `7823eb5` (2026-05-24)

##### TD4-NEW-12 — `flat_shading_bit_pinned` test comment stale post-#1190

- **ID**: TD4-NEW-12
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:334-343`
- **Evidence**: Test doc says `"the shader hard-codes (inst.flags & 128u)"`. After #1190,
  `triangle.frag` reads `(inst.flags & INSTANCE_FLAG_FLAT_SHADING)` via the generated header —
  `grep '128u' shaders/triangle.frag` returns nothing. The pin itself (asserting
  `INSTANCE_FLAG_FLAT_SHADING == 128`) remains valid as a bit-stability guard.
- **Fix**: Update comment to say the pin guards the *named constant's bit position*; the shader no
  longer uses `128u` directly.
- **Effort**: trivial

##### TD4-NEW-13 — `skin_vertices.comp` workgroup size has no lockstep test

- **ID**: TD4-NEW-13
- **Location**: `crates/renderer/shaders/skin_vertices.comp:40`; `crates/renderer/src/vulkan/skin_compute.rs:1139-1148`
- **Evidence**: `skin_vertices.comp:40`: bare literal `64` for `local_size_x`. The test
  `skin_palette_workgroup_size_matches_skin_vertices` at `skin_compute.rs:1139` reads only
  `skin_palette.comp`, not `skin_vertices.comp`. Dispatch arithmetic `push.vertex_count.div_ceil(WORKGROUP_SIZE)` is load-bearing — if `local_size_x` drifts, dispatch is silently wrong.
- **Fix**: Extend the test to also `include_str!` `skin_vertices.comp` and assert it contains
  `local_size_x = {WORKGROUP_SIZE}`. Rename test to
  `both_skin_shaders_workgroup_size_matches_rust_constant`.
- **Effort**: trivial
- **Age**: `48646895` (2026-05-xx, Fix #1152)

##### TD4-NEW-15 — bsver 9 and 21 have no named constants in bsver module

- **ID**: TD4-NEW-15
- **Location**: `crates/nif/src/blocks/collision/collision_object.rs:67`; `crates/nif/src/blocks/properties.rs:73`; `crates/nif/src/version.rs` (bsver module)
- **Evidence**: `collision_object.rs:67`: `if stream.bsver() < 9` (pre-Oblivion-mainline bhkBlendCollisionObject floats). `properties.rs:73`: `if stream.bsver() > 21` (emissive-mult gate). The bsver module has named constants for 0, 11, 14, 24, 26, 28, 34 — not for 9 or 21. The two sites cannot currently reference named constants.
- **Fix**: Add `pub const PRE_OBLIVION_MAINLINE: u32 = 9;` and `pub const FO3_EMISSIVE_MULT: u32 = 21;` to the bsver module with nif.xml citations. Update the two call sites.
- **Effort**: small

#### Dimension 6 — Test Hygiene

##### TH6-NEW-01 — `dump_prospector_saloon_refrs` is an `#[ignore]`d test with zero assertions

- **ID**: TH6-NEW-01
- **Location**: `crates/plugin/tests/parse_real_esm.rs:1543-1632`
- **Evidence**: Function body consists entirely of `eprintln!` calls — a tabular dump. No `assert!`
  of any kind. Passes vacuously whenever game data is readable; cannot catch any regression.
  Module docstring (line 6) says tests assert per-category floors; this one does not.
- **Fix**: Add floor assertions (e.g. `assert!(rows.len() > 0, ...)` and `assert!(multi_axis > 0, ...)`) or move to a named example (`crates/plugin/examples/dump_cell_refrs.rs`) so it is clearly diagnostic, not a regression guard.
- **Effort**: trivial

##### TH6-NEW-02 — `cross_game_translation_completeness` defers all fill-rate floor assertions

- **ID**: TH6-NEW-02
- **Location**: `crates/nif/tests/translation_completeness.rs:197-262`
- **Evidence**: File-level comment: "assert that field fill-rates don't drift below thresholds."
  Actual hard assertion at line 257 fires only on buffer-length structural mismatches. Lines 243–248
  explicitly defer per-game fill-rate floors. A regression silently dropping UV fill-rate from 98%
  to 0% on FNV would still pass.
- **Fix**: Capture per-game fill-rate run as constants (e.g. `FNV_MIN_UV_FILL_RATE = 0.95`) and
  add `assert!(fill_rate >= MIN, ...)` for UV, normals, and tangents. Numbers are already computed
  in the existing `eprintln` table.
- **Effort**: small

##### TH6-NEW-03 — `cube_demo_60f.png` golden baseline stale — 76 shader commits since capture

- **ID**: TH6-NEW-03
- **Location**: `byroredux/tests/golden/cube_demo_60f.png` (last updated `f62d4bd5`, 2026-05-09)
- **Evidence**: `git log f62d4bd5..HEAD -- crates/renderer/shaders/` returns 76 commits. Significant
  changes: Disney diffuse port (#1249), anisotropic GGX (#1250/#1253/#1254), Gram-Schmidt TBN
  (#1148), UV-mirror handedness (#1104), BGSM spec-gloss→metallic-roughness, per-light
  `falloff_exponent`. Running the golden test with `--ignored` would likely produce a mismatch
  panic rather than a valid regression signal.
- **Fix**: Run `BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame` on the dev machine (RTX 4070 Ti) and commit the updated PNG. Add a ROADMAP note on expected re-baseline cadence.
- **Effort**: trivial (one command + commit)
- **Age**: `f62d4bd5` (2026-05-09)

#### Dimension 7 — Doc Rot

##### TD7-NEW-01 — GpuMaterial struct doc says "260 bytes" in 8 sites; actual size is 300 B

- **ID**: TD7-NEW-01
- **Location**: `crates/renderer/src/vulkan/material.rs:38,79,766,895,986,1027`; `crates/renderer/shaders/triangle.frag:100,108`
- **Evidence**: `material.rs:38`: `"std430 GPU-side material record. 260 bytes per material"`.
  `triangle.frag:100`: `"Mirrors the Rust GpuMaterial (260 B std430, 16 vec4 + 4 B trailing)"`.
  Struct grew to 300 B via #1147 (+20 B), #1248 (+4 B), #1249 (+12 B), #1250 (+4 B). The test
  `gpu_material_size_is_260_bytes` correctly asserts 304… wait, 300 — with a documented stale-name
  rationale at `material.rs:1152-1154`. These 8 code-level doc sites are untracked by #1287.
- **Fix**: Update all 8 occurrences to `"300 bytes"` (note history: 260 → 280 → 284 → 296 → 300).
  The test name may stay per the grep-anchor rationale.
- **Effort**: trivial

##### TD7-NEW-02 — GpuCamera doc says "288 bytes / six trailing vec4"; actual is 304 B / seven vec4

- **ID**: TD7-NEW-02
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:171-175`; `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:38-39`
- **Evidence**: `gpu_types.rs:174`: `"six trailing vec4 (6×16 = 96 B: …, sky_tint)"`. The struct has
  7 trailing vec4 fields (sun_direction added by #1210). Actual size = 3×64 + 7×16 = 304 B.
  `gpu_instance_layout_tests.rs:38-39`: surrounding doc comment still says "must stay 288 B."
  The test assertion body (53) is correct at 304 B. Not tracked in any open issue.
- **Fix**: Update `gpu_types.rs:171,174-175` to `"304 bytes"` and `"seven trailing vec4 (7×16 = 112 B: …, sun_direction)"`. Update `gpu_instance_layout_tests.rs:38-39` to match.
- **Effort**: trivial

##### TD7-NEW-03 — Deleted `Material::classify_pbr` cited as live in 4 doc comments

- **ID**: TD7-NEW-03
- **Location**: `crates/core/src/ecs/components/material.rs:396,610`; `crates/nif/src/import/material/mod.rs:989`; `crates/nif/src/import/material/legacy_pbr_translation_tests.rs:7`
- **Evidence**: `material.rs:396`: `"shared by Material::classify_pbr (per-frame draw build)"`.
  `material.rs:610`: `"the hard-coded lists in [Material::classify_pbr]"`.
  `mod.rs:989`: `"Material::classify_pbr and this importer-side translation stay in lockstep"`.
  `grep -rn 'fn classify_pbr\b'` returns zero hits — the method does not exist. Only the free
  function `classify_pbr_keyword` and `Material::resolve_pbr` remain. Not tracked in any open issue.
- **Fix**: At each of the four sites, replace `Material::classify_pbr` with the actual live symbol
  (`classify_pbr_keyword` free function). E.g. `material.rs:396`: `"shared by classify_pbr_keyword (delegated to by Material::resolve_pbr at the NIFAL boundary)"`.
- **Effort**: trivial

##### TD7-NEW-04 — `material.rs` cites stale `triangle.frag:83-126` line range; actual 110–184

- **ID**: TD7-NEW-04
- **Location**: `crates/renderer/src/vulkan/material.rs:1183,1190,1244`
- **Evidence**: Three comments cite `triangle.frag:83-126` for the `struct GpuMaterial` location.
  The struct currently starts at line 110 and ends at line 184 (Disney BSDF additions #1248–1250
  grew the struct and shifted its close brace from ~126 to ~184).
- **Fix**: Update the three occurrences to `triangle.frag:110-184` (or drop line-number annotations
  and rely on a grep-anchor comment per the test doc at lines 1188-1190).
- **Effort**: trivial

#### Dimension 8 — API Surface / Re-Export Hygiene

##### TD8-D8-NEW-01 — `pub use legacy::{LegacyFormId, LegacyLoadOrder}` has zero external consumers

- **ID**: TD8-D8-NEW-01
- **Location**: `crates/plugin/src/lib.rs:35`
- **Evidence**: Workspace-wide grep finds zero callers outside `crates/plugin/src/legacy/mod.rs`
  (definition) and `lib.rs:35` (re-export). No binary, crate, or integration test imports either
  type from `byroredux_plugin::`. Both types introduced `bed67b87b` (2026-03-28); the re-export was
  never consumed. CLAUDE.md notes "Per-game parser stubs were removed under #390"; the live ESM
  path is in `crates/plugin/src/esm/`.
- **Fix**: Delete `pub use legacy::{LegacyFormId, LegacyLoadOrder};` from `lib.rs:35`. Types remain
  accessible internally. Re-add if a concrete external consumer lands.
- **Effort**: trivial
- **Age**: `bed67b87b` (2026-03-28)

#### Dimension 9 — File / Function / Module Complexity

##### TD9-NEW-02 — `asset_provider.rs` crossed 2000-LOC ceiling (+746 LOC since 2026-05-22)

- **ID**: TD9-NEW-02
- **Location**: `byroredux/src/asset_provider.rs`
- **Evidence**: 2561 LOC today; was 1815 in the 2026-05-22 audit (watchlist band). Growth: BGSM
  cycle resolver (#1148), FO4 CDB/StarfieldPhase1 material wiring (#1289), BSGeometry external
  mesh path (#1292), BGSM spec-to-roughness translation. Three logically distinct providers exist:
  `TextureProvider` (lines 55–666 + MeshResolver impl), `MaterialProvider` (lines 667–1397),
  37 inline unit tests (lines 1398–2561). Nesting depth 8 at lines 812–817.
- **Fix**: Split into `byroredux/src/asset/texture_provider.rs`, `material_provider.rs`,
  `mesh_resolver.rs`, `tests.rs`. The `Archive` helper struct (lines 18–54) → `asset/archive.rs`
  or inline in `texture_provider.rs`. No Vulkan surfaces; splittable today.
- **Effort**: small
- **Age**: `9aa69c68` (most recent, 2026-05-28)

##### TD9-NEW-03 — `commands.rs` crossed 2000-LOC ceiling with 23 command structs in one file

- **ID**: TD9-NEW-03
- **Location**: `byroredux/src/commands.rs`
- **Evidence**: 2115 LOC; not present as a >2000 file in any prior audit. Contains 23 independent
  `ConsoleCommand` impls. `MeshInfoCommand::execute()` is 238 LOC (line 247). Nesting depth 9
  at line 1356–1362.
- **Fix**: Split by command domain: `byroredux/src/commands/scene_cmds.rs` (tex.*, mesh.*),
  `camera_cmds.rs` (cam.*), `debug_cmds.rs` (stats, entities, systems, sys_accesses, ctx_scratch,
  mem_frag), `actor_cmds.rs` (skin.*, prid, near, pick, script_activate, door_teleport),
  `registry.rs` (build_command_registry, CommandRegistry). No Vulkan surfaces; splittable today.
- **Effort**: small
- **Age**: `3ce98db8` (most recent, 2026-05-28)

##### TD9-NEW-04 — `main.rs` fn `new()` is 504 LOC; file crosses #1267 promotion threshold at 2448 LOC

- **ID**: TD9-NEW-04
- **Location**: `byroredux/src/main.rs:403`
- **Evidence**: `main.rs` is 2448 LOC (+286 vs 2026-05-24). Issue #1267 stated "Promote when
  main.rs crosses 2400 LOC before a split lands." `fn new()` lines 403–906 = 504 LOC (ECS
  bootstrap + CLI argument parsing + BSA/ESM loading + NIF scene loading + system registration).
  `fn about_to_wait()` 353 LOC. `fn render_one_frame()` 269 LOC. Nesting depth 9 at lines 1442–1452.
- **Fix**: Extract: `byroredux/src/init/world_bootstrap.rs`, `byroredux/src/init/cli.rs`,
  `byroredux/src/init/scene_loader.rs`. Move `render_one_frame` → `byroredux/src/render.rs`
  (already exists). `main.rs` shrinks to thin `App` struct + `ApplicationHandler` delegation.
  No Vulkan API surfaces; splittable today.
- **Effort**: medium
- **Age**: `9aa69c68` (most recent, 2026-05-28)

##### TD9-NEW-05 — `parse_esm_with_load_order` is 879 LOC with a 109-arm match

- **ID**: TD9-NEW-05
- **Location**: `crates/plugin/src/esm/records/mod.rs:119`
- **Evidence**: Function body lines 119–997 = 879 LOC. Contains: file-header parse, GameKind
  derivation, 13 scratch HashMaps, localized-flag RAII guard, FO4 gate, then a while-loop with a
  109-arm match on 4-char record-type labels. ~49 arms contain inline logic (SCOL/PKIN/MOVS/MSWP
  FO4 gate with warn-once latches, WRLD child dispatch, LGTM sub-record extraction). Nesting depth
  6. Refactor ac7d0d5 (#1118 partial) split `index.rs` + `grup_walker.rs` but the main dispatch fn
  remained monolithic.
- **Fix**: Promote the match body to a `GrupDispatcher` struct (or table of fn pointers keyed by
  `RecordType`). Each GRUP type gets its own `fn parse_<type>_grup(reader, end, index) → Result<()>`.
  The 13 scratch HashMaps fold into an `EsmScratch` struct. Main walk-loop collapses to ~3 lines.
  Each GRUP independently testable.
- **Effort**: medium
- **Age**: `7ffda15d` (most recent, 2026-05-29)

##### TD9-NEW-06 — `recreate_swapchain` is 669 LOC in `resize.rs`

- **ID**: TD9-NEW-06
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:20`
- **Evidence**: `resize.rs` total 750 LOC; `recreate_swapchain` spans lines 20–688 = 669 LOC.
  Three distinct phases: (1) destroy old framebuffers + depth + swapchain (~lines 20–120);
  (2) create new swapchain + views + depth + framebuffers (~120–450);
  (3) conditional pipeline + render-pass rebuild when format changes (~450–688). Nesting depth 7
  at lines 285–286 and 493–494.
- **Fix**: Extract three private helpers: `fn destroy_old_swapchain_resources(&mut self)`,
  `fn create_new_swapchain_resources(&mut self, window_size)`,
  `fn rebuild_format_dependent_pipelines(&mut self, old_fmt, new_fmt)`.
  `recreate_swapchain` becomes a 30-LOC orchestrator. Flag: requires RenderDoc capture before/after
  to verify pixel equality (per `feedback_speculative_vulkan_fixes.md`).
- **Effort**: small (blocked on RenderDoc capture)
- **Age**: `06af4698` (most recent, 2026-05-28)

##### TD9-NEW-07 — `parse_nif_with_options` is 587 LOC with three phases already labelled

- **ID**: TD9-NEW-07
- **Location**: `crates/nif/src/lib.rs:188`
- **Evidence**: `lib.rs` 864 LOC; `parse_nif_with_options` spans lines 188–774 = 587 LOC. Phase 1
  (header, 188–206), Phase 2 (block loop + drift detection, 207–694), Phase 3 (root identification,
  695–774) already labelled with `// Phase N` comments. Nesting depth 8 at lines 425–452.
- **Fix**: Extract `fn parse_header(...)`, `fn parse_blocks(...)`, `fn identify_root(...)`.
  `parse_nif_with_options` becomes a 20-LOC orchestrator. `DriftStats` is an existing local struct.
  No external API changes.
- **Effort**: small
- **Age**: `6368b077` (most recent, 2026-05-28)

##### TD9-NEW-08 — `import_embedded_animations` is 424 LOC with nested local fn definitions

- **ID**: TD9-NEW-08
- **Location**: `crates/nif/src/anim/entry.rs:114`
- **Evidence**: `entry.rs` 537 LOC; `import_embedded_animations` lines 114–537 = 424 LOC. Contains
  two nested local `fn` definitions: `net_of` (53 LOC, downcast chain across 8 block types) and
  `walk_controller_chain` (250 LOC recursive controller-type dispatcher). The nested-fn pattern
  prevents separate testing.
- **Fix**: Promote `net_of` → `crates/nif/src/anim/net_of.rs` (or trait extension on `NiObject`).
  Promote `walk_controller_chain` → `crates/nif/src/anim/controller_walk.rs`.
  `import_embedded_animations` becomes ~50-LOC orchestrator. Each phase independently testable.
- **Effort**: small
- **Age**: `fe47706f` (anim split, 2026-05-14)

---

### INFO

#### TH6-NEW-04 — Stated `#[ignore]` baseline count (115) overstates true figure by ~19

- **ID**: TH6-NEW-04
- **Location**: Audit baseline vs codebase
- **Evidence**: Baseline captured as `grep -rn '#[ignore]' | wc -l = 115`, which counts doc-comment
  lines containing the text (e.g. `archive/tests.rs:521 // '#[ignore]'d so CI stays green`).
  Attribute-position grep (`^\s*#\[ignore`) yields **96** actual ignore attributes: 82 bareword
  `#[ignore]` and 14 string-form `#[ignore = "..."]` (13 in `skinning_e2e.rs`, 1 in
  `golden_frames.rs`). True count: **96**.
- **Fix**: Update the baseline collection script to use `grep -rEn '^\s*#\[ignore'` so future
  audits compare against the true attribute count.
- **Effort**: trivial

#### TD4-NEW-14 — Issue #1119 marks TD4-207 as deferred but it was fixed in commit 48646895

- **ID**: TD4-NEW-14
- **Location**: `.claude/issues/1119/ISSUE.md` (Deferred section: TD4-207)
- **Evidence**: ISSUE.md states TD4-207 deferred — `"Caustic / SSAO / SVGF-temporal / TAA compute
  shaders still use bare 8; blocked on shader-compile environment"`. Running `grep local_size` on
  all four shaders shows they use `WORKGROUP_X`/`WORKGROUP_Y` (not bare 8). Commit `48646895`
  message: `"Fix #1152: compute shaders use WORKGROUP_X/Y instead of hardcoded 8"`. Issue #1152
  (= TD4-303) was the fix.
- **Fix**: Update `.claude/issues/1119/ISSUE.md`: move TD4-207 from Deferred to Closed, cite
  commit `48646895` / issue #1152.
- **Effort**: trivial

---

## 6. Deferred — Findings Gated on Open Milestones

### TD5-NEW-02 *(DEFERRED — LOW)* — cxx-bridge native_hello / engine_info: greeting-only C++ stub called at every engine startup

- **ID**: TD5-NEW-02
- **Location**: `crates/cxx-bridge/src/lib.rs:17-35`; `byroredux/src/main.rs:173`
- **Evidence**: Entire C++ surface (`native_utils.cpp`) is a `"Hello from C++ side of ByroRedux!"`
  string return. Called unconditionally at every engine start. `engine_info()` defined but return
  value not consumed. Scaffolded 2026-03-29 (`a26afd41`), never extended.
- **Fix Option A**: Replace with the first real C++ interop surface when C++ integration begins.
  **Fix Option B (immediate)**: Add an explicit `// Intentional placeholder until C++ integration
  (milestone M-XY) begins` comment at the call site and in `lib.rs`.
- **Effort**: trivial (Option B); blocked on first real C++ milestone for Option A
- **Deferred because**: No C++ milestone is currently scheduled. Option B is available immediately
  and is recommended to avoid re-surfacing in future audits.
- **Age**: `a26afd41` (2026-03-29)

### TD9-200 / TD9-201 *(carry, BLOCKED — see MEDIUM above)*

Also deferred in the sense that the split is blocked on the RenderDoc captured-frame baseline
precondition. Classified MEDIUM rather than deferred because the file growth is active and the
unblocking action is well-defined (and high-leverage).

---

## 7. Checked-and-Clean Items

The following were verified against the current tree. The next audit may skip them unless a relevant
commit appears in the intervening diff.

### Dimension 1 — Stale Markers

- All 4 markers verified (grep `-RInE '(TODO|FIXME|HACK|XXX)\b'`). Baseline holds at 4.
- `crates/renderer/src/vulkan/material.rs:593` — ACTIVE TODO: `'not yet plumbed into our GpuMaterial — left as a TODO for when the transmission lobe lands (#1248-followup)'`. Age 6 days (`c09d63a6`, 2026-05-23). Under 30-day skip threshold; parent issue #1248 open. Reviewed as properly tracked in AUDIT_TECH_DEBT_2026-05-24.md Dim 1 table.
- `crates/bgsm/src/bgem.rs:122` — PROSE REFERENCE, not an active marker. Text: `'Order matches the reference's // FIXME note'` — describes an external Bethesda source comment. Age 39 days (`edb0525e`, 2026-04-20). Not actionable.
- `crates/nif/src/blocks/bs_geometry.rs:563` — PROSE REFERENCE. Text: `'Per the FIXME at BSGeometryMeshData::Sync line 1709'` — cites an external Bethesda comment. Age 33 days (`f8315a1b`, 2026-04-26). Not actionable.
- `byroredux/src/scene.rs:794` — CLOSURE MENTION. Text: `'Closes the #242 consumer-side TODO (#1055)'` — documents a resolved issue. Age 15 days (`e6192cc5`, 2026-05-14). Not actionable.
- Disney-BSDF attribution block (`triangle.frag` lines 11–30, GLSL-PathTracer MIT + Burley 2012): INTACT. Added by `9451ce3c` (2026-05-28). MIT notice complete, URL present, Burley 2012 cited.
- No markers older than 6 months. No markers reference closed issues. Count steady at 4.

### Dimension 2 — Dead Code

- All 27 `#[allow(dead_code)]` annotations verified. No unjustified suppressions.
- `byroredux/src/components.rs:227-265` — 9 extended `CellLightingRes` fields: intentional staged-shader-rollout markers per comments; issues #861/#865 corroborate the plan.
- `crates/bsa/src/ba2.rs:148-150` — `Dx10Chunk.start_mip` / `end_mip`: annotated for M40 streaming pre-wiring (#1049); justified.
- `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:212-241` — `VF_UVS_2`, `VF_LAND_DATA`, `VF_INSTANCE`: schema-completeness constants per #336/#358; intentional.
- `crates/plugin/src/esm/records/mswp.rs:150` — `peek_path_filter`: reserved for FO4-DIM6-02 stage-2; appropriately annotated.
- `crates/plugin/examples/sf_smoke.rs:114` — `WalkReport.tes4_bytes`: micro-smell (field on a struct no consumer pattern-matches) but consistent with the "kept on struct for diffing" intent; trivially harmless in an example binary.
- `crates/plugin/src/manifest.rs:73` — `RawDependency.name`: silences serde deserialization of a TOML key discarded after parsing; standard pattern.
- `crates/renderer/src/vulkan/scene_buffer/buffers.rs:25` — `LightHeader.count`: GPU-FFI write-only; never read back. Correct annotation.
- `byroredux/src/main.rs:378` — `debug_server` field: `cfg(feature="debug-server")` gated; hold-for-Drop pattern. Correct.
- `byroredux/src/cell_loader/refr.rs:64` — `inner: Option<FixedString>`: preserved for slot-index=6 XTXR swap round-trip parity; justified.
- `byroredux/src/components.rs:768` — `SkyDomeRes::texture_indices()`: reserved for worldspace-transition release hook (#1199); justified.
- `crates/core/src/ecs/access.rs:255,267` — `Health(f32)` and `Clock(f32)`: test-module fixture types; `#[allow(dead_code)]` inside `#[cfg(test)]` is the standard pattern.
- `debug-ui` crate: `DebugUiState`, `PanelState`, `PanelOutputs`, `PanelSnapshot`, `QueuedLoad`, `CONSOLE_HISTORY_CAP`, and all methods actively used from `byroredux/src/main.rs`.
- `sfmaterial` crate: `ComponentDatabaseFile` actively parsed and stored (`asset_provider.rs`); Phase 1 presence-only documented.

### Dimension 3 — Duplication

- NIF base-class scaffolding (`NiObjectNETData::parse`, `NiAVObjectData::parse`, `BSShaderPropertyData::parse_base`): fully consolidated in `crates/nif/src/blocks/base.rs`; all block parsers delegate correctly.
- `impl_ni_object!` macro: 30 uses across block files; zero manual `NiBlock` impls outside the macro.
- ESM `for sub in subs { match sub_type }` loops (145 sites): correct pattern using `find_sub` / `read_*_sub` helpers from `records/common.rs`; no duplication.
- `cell/helpers.rs::read_form_id` and `read_form_id_array`: NOT duplicates of anything in `records/common.rs` (different signatures — take `&[u8]` directly); should stay.
- `EXTERIOR_CELL_UNITS = 4096.0`: fully consolidated in `crates/core/src/math/coord.rs:41`.
- `cell_grid_to_world_yup`: single canonical implementation; all callers use the helper.
- Vulkan `memory_barrier` helper (`crates/renderer/src/vulkan/descriptors.rs:139`): adopted by all 13 global-barrier sites.
- `image_barrier_undef_to_general` helper: adopted by all `UNDEFINED→GENERAL` init paths; zero remaining inline builders.

---

*Audit date: 2026-05-28. Auditor: audit-tech-debt skill (11-dimension sweep). Next sweep recommended
after the next major milestone landing or within 7 days, whichever comes first.*
