# Tech-Debt Audit — 2026-06-14

9-dimension sweep (orchestrator + per-dimension Task agents). Prior report:
[2026-05-28](AUDIT_TECH_DEBT_2026-05-28.md). Run as part of a `comprehensive`
`/audit-suite` sweep.

---

## 1. Executive Summary

**18 findings** across 9 dimensions — **0 HIGH, 1 MEDIUM, 17 LOW**. No
CRITICAL. The single MEDIUM (TD3-001) is a recurrence of the GpuCamera-size
doc-rot class (#1526/#1321) at a *new* site: a doc comment in
`context/mod.rs` asserts a 304-byte GpuCamera layout that is now pinned at
**336 B**.

The debt profile this cycle is dominated by **file/function growth**, not new
correctness rot. The two BLOCKED Vulkan-recording monoliths continued to grow:
`draw.rs` **3337 → 3831** (+494) and `context/mod.rs` **3017 → 3142** (+125).
`draw_frame()` is now a single **~3211-LOC function**. All three binary-crate
files that crossed 2000 LOC in the last cycle grew further
(`asset_provider.rs` 2561 → 2833, `commands.rs` 2115 → 2430, `main.rs` 2448 →
2661), and `particle.rs` (2125) is a **net-new** entrant to the >2000 set.

The recurring doc-rot and magic-number traps are otherwise **holding**: six
GPU-struct/Vertex/bone size claims verified still-correct against their pinned
tests; four magic-number dedup traps (#1336/#1482/#1357/#1348) confirmed
still-fixed; the #1318 logic-duplication closures (read_zstring, the four
consolidated coord-flip sites) confirmed still-fixed in production; the #1320
test-assertion closures confirmed not-regressed. The path-validation gate
passed clean (776 refs, 0 stale).

| Severity | NEW | Existing/Regression | Total | Dimensions |
|----------|-----|---------------------|-------|------------|
| CRITICAL | 0   | 0                   | 0     | — |
| HIGH     | 0   | 0                   | 0     | — |
| MEDIUM   | 1   | 0                   | 1     | D3 |
| LOW      | 13  | 4                   | 17    | D1, D2, D3, D4, D5, D6, D7, D8, D9 |

Delta vs 2026-05-28: the headline file-growth findings restate OPEN umbrella
#1323 with materially-worse LOC; the prior cycle's GPU-size/classify_pbr
quick-wins were closed in the intervening fixing sprint (#1321/#1349/#1357/
etc. now CLOSED) and verified still-fixed here.

---

## 2. Baseline Snapshot

Source: `/tmp/audit/tech-debt/baseline.txt` (captured 2026-06-14 pre-sweep).

| Metric | 2026-06-14 (today) | 2026-05-28 | Δ |
|---|---:|---:|---:|
| `TODO`/`FIXME`/`HACK`/`XXX` (raw grep) | 20 | 4 | +16 |
| ↳ of which *active production markers* | **3** | 1 | +2 |
| `#[allow(dead_code)]` | 31 | 27 | +4 |
| `unimplemented!()` / `todo!()` | **0** | 0 | 0 |
| `panic!("not …")` | 0 | 0 | 0 |
| `#[ignore]` tests (raw, excl. target/) | 247 | 115 | +132 |
| ↳ genuine debt (not Vulkan/data gate) | **1** | — | — |
| Files > 2000 LOC | **6** | 5 | +1 |
| `_audit-validate.sh` | **OK (776 refs, 24 files)** | not run | — |

> The raw-marker jump (4 → 20) is almost entirely the `XXXX` ESM extended-size
> protocol tag (reader.rs / records/misc/magic.rs) and upstream-FIXME
> references — both are documented false-positives, not debt. Active production
> markers: 3 (TD5-001/002/003). The `#[ignore]` jump (115 → 247) reflects raw
> grep counting attribute occurrences across the grown test corpus; 246 of 247
> are legitimate Vulkan/game-data gates (1 genuine debt = TD9-001).

**Files > 2000 LOC (current):**

| File | LOC (2026-06-14) | LOC (2026-05-28) | Δ |
|---|---:|---:|---:|
| `crates/renderer/src/vulkan/context/draw.rs` | **3831** | 3337 | +494 (BLOCKED) |
| `crates/renderer/src/vulkan/context/mod.rs` | **3142** | 3017 | +125 (BLOCKED) |
| `byroredux/src/asset_provider.rs` | **2833** | 2561 | +272 |
| `byroredux/src/main.rs` | **2661** | 2448 | +213 |
| `byroredux/src/commands.rs` | **2430** | 2115 | +315 |
| `crates/nif/src/blocks/particle.rs` | **2125** | <2000 | NEW crossing |

---

## 3. Top 10 Quick Wins

Trivial-to-small effort, immediate readability or correctness-of-docs payoff.
No Vulkan recording changes. Suggested execution order:

1. **TD3-001** *(MEDIUM)* — Fix stale "304 B" → "336 B" GpuCamera size in `context/mod.rs:678` (or reword to not pin a size). One line.
2. **TD8-001** — Delete 9 obsolete `#[allow(dead_code)]` on `CellLightingRes` fields (now read by `cell.lighting`). `components.rs:269-306`.
3. **TD8-002** — Delete stale `#[allow(dead_code)]` + "not yet consumed" comment on `RefrTextureOverlay::inner` (now used). `cell_loader/refr.rs:64`.
4. **TD3-002** — Reword `classify_legacy_pbr` doc to not name deleted `Material::classify_pbr` as a live per-frame entry point. `import/material/mod.rs:986-990`.
5. **TD3-003** — Update `has_packed_combined_geom_extra` doc: CSG loader LANDED (M49), drop "future work". `import/walk/mod.rs:46-49` (Existing: #1446).
6. **TD8-004** — Delete write-only `class_by_type_id` field + its insert. `sfmaterial/src/reader.rs:187,61` (Existing: #1324).
7. **TD8-003** — Delete dead `peek_path_filter` (zero callers, 3 audits pending) or add a tracking link. `mswp.rs:150-155`.
8. **TD7-001** — Replace three `bsver() == 0`/`> 0` literals with the existing-but-unused `bsver::PRE_BETHESDA`. controller/ni_tri_shape/particle.
9. **TD4-001** — Fix FNV skill ref `predicates.rs::blas_budget_bytes` → `compute_blas_budget`. `audit-fnv/SKILL.md:75`.
10. **TD7-002** — Add `CNTO_WIRE_SIZE = 8` const, reference at both CNTO gate sites. container.rs:88 / actor.rs:531.

---

## 4. Top 5 Medium Investments

File/function splits and consolidations. Listed by leverage.

1. **TD1-301** — Extract `draw_frame()` (~3211 LOC) into per-pass `record_*` methods. **BLOCKED** on a RenderDoc capture (no barrier/order changes; mechanical scope-extraction only). Highest single-function leverage in the codebase.
2. **TD1-302** — Split `context/mod.rs` (3142): move pure value-types (`DrawCommand`/`SkyParams`/`material_hash`/`to_gpu_material`) to `context/types.rs` (low-risk, ungated); phase-split `new()` (~914 LOC) — gated. **BLOCKED** on the same capture for the gated steps.
3. **TD1-303** — Split `asset_provider.rs` (2833): `archive.rs` / `texture.rs` / `material.rs` + move the ~1308-LOC in-file test module to sibling test files (drops prod well under 2000).
4. **TD1-305** — Split `commands.rs` (2430) by command group (diag / tex / camera / skin / render / script). Almost no test bulk — all 2430 is production logic.
5. **TD1-304 / TD1-306** — Split `main.rs` (2661; `about_to_wait` 363 LOC + `main` 340 LOC) into `app/` modules; split `particle.rs` (2125) by particle responsibility (emitters / modifiers / fields / colliders).

---

## 5. Findings

### MEDIUM

#### TD3-001: Stale GpuCamera size (304 B) in `context/mod.rs` doc comment
- **Severity**: MEDIUM (stale GpuCamera size in a doc comment — per Tech-Debt severity table)
- **Dimension**: Stale Documentation
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:678`
- **Status**: NEW (same doc-rot class as CLOSED #1526/#1321, recurring at a new site)
- **Effort**: trivial
- **Description**: The `sun_angular_radius` doc says the change "doesn't touch GpuCamera's **304 B** layout." GpuCamera is now **336 B** — authoritative pin `gpu_camera_is_336_bytes` at `scene_buffer/gpu_instance_layout_tests.rs:56`. The struct grew 304 → 320 (DOF) → 336 (`render_origin`, #1492).
- **Evidence**: Comment text "doesn't touch GpuCamera's 304 B layout" vs `gpu_types.rs:171` `GpuCamera (336 bytes…)` + the asserting pin `size_of::<GpuCamera>() == 336`.
- **Impact**: A reader or next auditor cross-checking GpuCamera size against this site reads a contradicting value — exactly the trap #1526/#1321 fixed elsewhere, resurfaced. Lockstep-drift bait.
- **Related**: #1526, #1321 (both CLOSED).
- **Suggested Fix**: Change "304 B" → "336 B", or reword to "reuses the existing `sky_tint.w` slot, no new field" to avoid pinning a size that drifts.

---

### LOW — Dimension 1 (File / Function / Module Complexity)

> All restate OPEN umbrella #1323 except TD1-306/307 (net-new). The two Vulkan
> monoliths are the long-standing BLOCKED pair (RenderDoc-capture gate per
> `feedback_speculative_vulkan_fixes.md` — no barrier/order changes proposed).

#### TD1-301: `draw_frame()` is a single ~3211-LOC function
- **Severity**: LOW · **Dimension**: File/Function Complexity · **Effort**: large (BLOCKED)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:269-3480`
- **Status**: Existing: #1323 (TD9-200, BLOCKED). File grew 3337 → 3831.
- **Description**: ~3211 of `draw.rs`'s 3831 lines are inside one function. Every per-pass edit touches one 3000-line scope; impossible to unit-test sub-phases.
- **Suggested Fix**: Extract per-pass private `record_*` methods (`record_gbuffer_geometry`, `record_rt_lighting`, `record_svgf_denoise`, `record_volumetrics`, `record_water`, `record_bloom`, `record_composite_taa`, `record_ui`); `draw_frame` becomes the acquire→submit→present skeleton. Mechanical scope-extraction only — gated on a RenderDoc capture before merge.

#### TD1-302: `context/mod.rs` 3142 LOC with `new()` ~914 LOC + `drop()` ~272 LOC
- **Severity**: LOW · **Dimension**: File/Function Complexity · **Effort**: large (BLOCKED for gated steps)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs` (`new()` 1404-2318; `drop()` 2660-2932; `material_hash` 504-621; `to_gpu_material` 386-504)
- **Status**: Existing: #1323 (TD9-201, BLOCKED). File grew 3017 → 3142.
- **Suggested Fix**: **Step 1 (low-risk, ungated):** move pure value-types (`DrawCommand`/`SkyParams`/`DofView`/`DrawCallStats`/`FrameTimings`/`ScreenshotHandle` + `to_gpu_material`/`material_hash`) to a sibling `context/types.rs`. **Steps 2-3 (BLOCKED):** phase-split `new()` into `init_device`/`init_swapchain_and_passes`/`init_pipelines`/`init_descriptors`/`init_scene_buffers`; mirror in `Drop`. Preserve all safety comments.

#### TD1-303: `asset_provider.rs` 2833 LOC (prod ~1525)
- **Severity**: LOW · **Dimension**: File/Module Complexity · **Effort**: medium
- **Location**: `byroredux/src/asset_provider.rs:1-2833` (prod ends ~1525; test module 1525-2833)
- **Status**: Existing: #1323 (TD9-NEW-02). Grew 2561 → 2833.
- **Suggested Fix**: `asset_provider/mod.rs` (shared path utils) → `archive.rs` (BSA/BA2 `Archive` + `open_with_numeric_siblings`) → `texture.rs` (`TextureProvider`/`MeshResolver`) → `material.rs` (`MaterialProvider` + BGSM/BGEM/CDB chain). Move each `#[cfg(test)]` group to a sibling test file — drops prod well under 2000 by itself.

#### TD1-304: `main.rs` 2661 LOC; `about_to_wait` 363 LOC + `main()` 340 LOC + `render_one_frame` 307 LOC
- **Severity**: LOW · **Dimension**: File/Function Complexity · **Effort**: medium
- **Location**: `byroredux/src/main.rs:1-2661` (`about_to_wait` @2059; `main()` @109; `render_one_frame` @1444)
- **Status**: Existing: #1323 (TD9-NEW-04; note `new()` shrank 504 → 231, partly addressing that sub-item; new worst offenders are `about_to_wait`/`main`).
- **Suggested Fix**: `main.rs` keeps `main()`/`init_tracing` → `app/mod.rs` (App + Drop + new) → `app/event_loop.rs` (`ApplicationHandler` + `render_one_frame`) → `app/streaming.rs` (step_* helpers) → `boot.rs` (arg/profile expansion) → `script_shims.rs` (the six `*_dispatch` fns).

#### TD1-305: `commands.rs` 2430 LOC, 27 command structs, ~all production
- **Severity**: LOW · **Dimension**: File/Module Complexity · **Effort**: medium
- **Location**: `byroredux/src/commands.rs:1-2430` (`register_all` 2398-2424; `#[cfg(test)]` only at 2428)
- **Status**: Existing: #1323 (TD9-NEW-03). Grew 2115 → 2430. Only oversized file with no test-bulk excuse.
- **Suggested Fix**: `commands/mod.rs` (`register_all` + shared helpers) → `diag.rs` / `tex.rs` / `camera.rs` / `skin.rs` / `render.rs` / `script.rs` by command group.

#### TD1-306: `particle.rs` 2125 LOC (prod ~1359) — net-new oversized entrant
- **Severity**: LOW · **Dimension**: File/Module Complexity · **Effort**: medium
- **Location**: `crates/nif/src/blocks/particle.rs:1-2125` (`#[cfg(test)]` @1360; tests ~766 LOC)
- **Status**: NEW (not individually itemized in #1323).
- **Description**: ~40 `parse_*` modifier/emitter fns + typed emitter/modifier/force-field structs + the opaque `NiPSysBlock` fallback, all flat. Prod (~1359) is itself approaching the ceiling.
- **Suggested Fix**: `blocks/particle/mod.rs` (re-exports + `NiPSysModifierBase` + `NiPSysBlock` fallback) → `emitters.rs` / `modifiers.rs` / `fields.rs` (`NiPSysGravity/Vortex/Drag/Turbulence/AirField`) / `colliders.rs`; move test clusters to sibling files.

#### TD1-307: NIF block dispatcher `match type_name` has ~265 arms
- **Severity**: LOW · **Dimension**: Function/Match Complexity · **Effort**: large (and arguably intentional)
- **Location**: `crates/nif/src/blocks/mod.rs:264`
- **Status**: NEW.
- **Description**: The central type→parser dispatcher matches ~265 string arms (5× the >50 threshold). Caveat: it is a flat name→parser table, not logic-heavy; the arm count tracks legitimate format coverage (Oblivion→Starfield).
- **Suggested Fix**: Optional. If split, group into per-family helpers (`dispatch_geometry`/`dispatch_property`/`dispatch_collision`/`dispatch_particle`/`dispatch_animation`) delegated by prefix. Lower priority than the file splits — recommend leaving as-is unless it actively blocks edits.

---

### LOW — Dimension 2 (Logic Duplication)

#### TD2-001: Z-up→Y-up coord-flip reimplemented inline across mesh-decode + Havok import paths
- **Severity**: LOW · **Dimension**: Logic Duplication · **Effort**: small-medium
- **Location**: `crates/nif/src/import/mesh/sse_recon.rs:263,283,372`; `mesh/tangent.rs:88-89,92,254`; `mesh/skin.rs:480`; `import/collision.rs:797,803,~828`
- **Status**: NEW (the #1044/#1318 consolidation explicitly scoped only `import/coord`, `anim/coord`, `cell_loader/euler`, `spt::import` — these mesh/collision sites were never in scope; **not a regression**).
- **Description**: The canonical `(x,y,z)→(x,z,-y)` swap has a single source of truth (`byroredux_core::math::coord::zup_to_yup_pos` for vectors, `zup_to_yup_quat_wxyz` for quats, `import/coord.rs::zup_matrix_to_yup_quat` for the matrix-conjugation path). The SSE-reconstruction decoder, tangent extractor, skin-bone-matrix builder, and the whole Havok→engine path each re-spell the swap inline. `havok_quat_to_engine` even silently diverges from canonical by lacking the `#333` `normalize_quat` (benign today — Havok quats are unit by construction — hence LOW not MEDIUM).
- **Evidence**: Canonical `zup_to_yup_pos(p) = [p[0], p[2], -p[1]]` vs `sse_recon.rs:263` `positions.push([x, z, -y])` (identical); canonical quat `[x, z, -y, w]` (normalized) vs `collision.rs:803` `Quat::from_xyzw(q[0], q[2], -q[1], q[3])` (missing normalize).
- **Impact**: Any future coord-convention tweak (or a fix like #333) must be hand-applied to ~9 sites.
- **Suggested Fix**: Route vector sites through `byroredux_core::math::coord::zup_to_yup_pos`; route `havok_quat_to_engine` through `zup_to_yup_quat_wxyz` (picks up `#333` normalize); route `decompose_havok_matrix` rotation extraction through `import/coord.rs::zup_matrix_to_yup_quat`.

---

### LOW — Dimension 3 (Stale Documentation)

#### TD3-002: Deleted `Material::classify_pbr` named as a live per-frame entry point
- **Severity**: LOW · **Dimension**: Stale Documentation · **Effort**: trivial
- **Location**: `crates/nif/src/import/material/mod.rs:986-990`
- **Status**: NEW (same class as CLOSED #1321/#1522, new site).
- **Description**: `classify_legacy_pbr`'s doc says it stays "in lockstep" with "the per-frame draw build's `Material::classify_pbr`." That method was deleted in the NIFAL refactor — PBR resolves at the parse-time `translate_material` boundary; there is no per-frame classifier. (`grep "fn classify_pbr\b"` → zero hits.) The sibling doc at `crates/core/src/ecs/components/material.rs:410-411` correctly calls it "(deleted)".
- **Suggested Fix**: Reword to reference the live `classify_pbr_keyword` free fn and note `Material::classify_pbr` was removed in the NIFAL refactor.

#### TD3-003: CSG-loader doc framed as future/unbuilt work after M49 landed
- **Severity**: LOW · **Dimension**: Stale Documentation · **Effort**: trivial
- **Location**: `crates/nif/src/import/walk/mod.rs:46-49`
- **Status**: Existing: #1446 (OPEN).
- **Description**: `has_packed_combined_geom_extra` doc says "Future CSG-loader work will populate these … until then the cell-loader falls back to per-REFR rendering." The M49 CSG reader landed (`crates/bsa/src/csg.rs` `CsgArchive`, driven by `cell_loader/precombined.rs`, whose own header already says "M49 — complete"). This is a missed sibling site.
- **Suggested Fix**: Update to reflect M49: `CsgArchive` now populates these from the geometry `.csg`; per-REFR fallback fires only when CSG spawn count is zero.

---

### LOW — Dimension 4 (Audit-Finding Rot)

#### TD4-001: FNV skill cites non-existent function `predicates.rs::blas_budget_bytes`
- **Severity**: LOW · **Dimension**: Audit-Finding Rot · **Effort**: trivial
- **Location**: `.claude/commands/audit-fnv/SKILL.md:75`
- **Status**: NEW.
- **Description**: The skill attributes the BLAS budget formula (`device_local_bytes / 3` floored at `MIN_BLAS_BUDGET_BYTES`) to `predicates.rs::blas_budget_bytes`. That is not a function — the formula lives in `compute_blas_budget` (`acceleration/predicates.rs:547`); `blas_budget_bytes` is only the struct *field* (`acceleration/mod.rs:153`) that caches the result.
- **Impact**: A future audit grepping `predicates.rs` for the function will not find it and may conclude the budget logic was removed. (The path gate cannot see symbol-anchor drift.)
- **Suggested Fix**: Change the ref to `predicates.rs::compute_blas_budget`.

> Dim-4 coverage note: ~30 symbol-anchor refs, ~14 issue-state refs, 12
> dimension counts, the crate-count=19 baseline, and report ages (oldest 73d,
> none >90d) were spot-checked — only the one above was stale. All sampled
> `#NNN` skill refs are correctly-closed regression pins.

---

### LOW — Dimension 5 (Stale Markers)

> Zero `unimplemented!`/`todo!()`/`panic!("not …")` in production (baseline
> holds). 3 active production markers; all are recent (9-22 days) and none
> names an *open* driving issue. The `triangle.frag` MIT/Burley attribution
> header is intact (must-not-delete).

#### TD5-001: GI hit-normal SSBO fetch documented as "mis-resolved" — bounce uses a stand-in normal
- **Severity**: LOW · **Dimension**: Stale Marker · **Effort**: medium · **Age**: commit 6ac502ac8, 2026-06-05
- **Location**: `crates/renderer/shaders/triangle.frag:3596`
- **Status**: Active marker, no tracking issue.
- **Description**: The 1-bounce GI path approximates the receiver normal as `-giDir` because "the SSBO normal/position fetch mis-resolved here." Unlike the other two markers (unbuilt features), this documents a *known-broken* fetch. Visual impact is "less sharp colour bleed," not "wrong colour" (defensible 1-spp choice) — hence LOW.
- **Suggested Fix**: Open a tracking issue for the SSBO normal/position fetch mis-resolution so the marker has a driver, or downgrade wording to "not yet wired" if the fetch was never attempted.

#### TD5-002: `GpuMaterial::glass()` transmission TODO names a CLOSED issue; preset has zero callers
- **Severity**: LOW · **Dimension**: Stale Marker · **Effort**: small (retarget) / large (implement) · **Age**: commit c09d63a6f, 2026-05-23
- **Location**: `crates/renderer/src/vulkan/material.rs:602`
- **Status**: Active marker; references `#1248-followup` where **#1248 is CLOSED**, no replacement tracker. `GpuMaterial::glass` has zero call sites.
- **Description**: "spec_trans = 1.0 … left as a TODO for when the transmission lobe lands (#1248-followup)." Closed driver + unused preset = marker-outlived-its-driver. The sibling `car_paint()`/`metal()` presets carry the same "Disney extension not yet on GpuMaterial" note.
- **Suggested Fix**: File one tracking issue for the missing Disney lobes (transmission + clearcoat) and retarget all three preset comments, or drop the `#1248-followup` parenthetical. Do not delete the comments — they correctly document GpuMaterial's missing fields.

#### TD5-003: GI bounce albedo uses material tint only — texture-average fold-in TODO, untracked
- **Severity**: LOW · **Dimension**: Stale Marker · **Effort**: medium · **Age**: commit 6ac502ac8, 2026-06-05
- **Location**: `byroredux/src/render/static_meshes.rs:656`
- **Status**: Active marker, no tracking issue.
- **Description**: `avg_albedo` (the GI colour-bleed source) is filled from `diffuse_color`; the TODO notes textured surfaces should bounce a texel-mean (1×1 average at asset load). Enhancement, not a bug — the current value is exact for untextured/vertex-coloured surfaces and the correct tint for textured content (an improvement over the prior hardcoded 0.5 grey).
- **Suggested Fix**: File a tracking issue (texel-mean at texture upload) and reference it, or leave as a well-scoped honestly-documented enhancement.

---

### LOW — Dimension 6 (Stub & Placeholder Implementations)

> 24 stub/placeholder sites triaged; 23 are documented intentional fallbacks
> (block_size recovery, Havok constraint-CInfo stub #117, FO4+ `NewPhysicsStub`
> PHYSAL gap, SpeedTree billboard #997, IMGS/LGTM/MNAM best-effort capture,
> no-op tracing subscriber, transient-image SAFETY comments) — **not debt**.

#### TD6-001: RACE `DATA` parser applies the TES4/FO3/FNV 36-byte layout to Skyrim's 128/164-byte DATA
- **Severity**: LOW · **Dimension**: Stub/Placeholder · **Effort**: medium
- **Location**: `crates/plugin/src/esm/records/actor.rs:788` (arm at 789)
- **Status**: NEW — genuine bounded gap (documented but **length-gated, not game-gated**, so it mis-parses rather than skipping).
- **Description**: The `b"DATA" if sub.data.len() >= 36` arm has no `GameKind` check. A Skyrim RACE DATA (128/164 B) satisfies `len >= 36` and is decoded with the TES4-era layout → garbage skill bonuses / height / weight / flags. The comment honestly says "TES5 DATA is 128/164 bytes with a different layout — not yet wired here," but the guard is length-based, so it produces *plausible-but-wrong* values instead of leaving defaults. Reachable via `--master Skyrim.esm` and the `m41-equip.sh` smoke test.
- **Impact**: Bounded today — `RaceRecord` skill_bonuses/base_height/base_weight are not yet consumed by rendering/equip, so no visible symptom. Becomes a live foot-gun the moment a consumer reads those fields (returns wrong data, not "unknown").
- **Suggested Fix**: Gate the arm to `Oblivion | Fallout3NV` (e.g. `b"DATA" if !is_skyrim && sub.data.len() >= 36`); for Skyrim either parse the TES5 layout or leave the fields at defaults so a future consumer sees "unknown" not "garbage."

---

### LOW — Dimension 7 (Magic Numbers)

> 4 dedup traps (#1336 bare BSVER, #1482 DBG_* bits, #1357 BGSM aliases, #1348
> NUM_RESERVOIRS) verified still-fixed. GPU size literals all route through
> `size_of::<>()` + named `MAX_*`; shader `#define` provenance clean (DBG_BITS
> catalog pins all 13).

#### TD7-001: `bsver() == 0` / `> 0` should use the existing unused `bsver::PRE_BETHESDA`
- **Severity**: LOW · **Dimension**: Magic Numbers · **Effort**: trivial
- **Location**: `crates/nif/src/blocks/controller/mod.rs:465`; `tri_shape/ni_tri_shape.rs:408`; `particle.rs:1131`
- **Status**: NEW (adjacent to CLOSED #1336, which covered decimal *threshold* literals, not the `0` sentinel).
- **Description**: `bsver::PRE_BETHESDA: u32 = 0` (version.rs:289) was added expressly to name this sentinel but has **zero usages**. Three sites that test exactly "is this pre-Bethesda?" compare against the bare `0` instead; controller/mod.rs:465 even spells out the nif.xml `#BSVER# #EQ# 0` mapping in a comment.
- **Suggested Fix**: Replace the three literals with `bsver::PRE_BETHESDA`, retiring the dead constant into use.

#### TD7-002: CNTO subrecord size `>= 8` duplicated across two record parsers
- **Severity**: LOW · **Dimension**: Magic Numbers · **Effort**: trivial
- **Location**: `crates/plugin/src/esm/records/container.rs:88`; `records/actor.rs:531`
- **Status**: NEW.
- **Description**: CNTO (u32 FormID + i32 count = 8 B) is parsed identically in two files, both guarding on the bare `>= 8`. actor.rs:530 comment "CNTO: shared with CONT" acknowledges the coupling but no shared constant exists. The codebase already has the `WIRE_SIZE` convention (`InfoSet::WIRE_SIZE`, `WeatherColor::WIRE_SIZE`).
- **Suggested Fix**: Add `const CNTO_WIRE_SIZE: usize = 8;` near the shared inventory-entry types and reference at both gate sites.

---

### LOW — Dimension 8 (Dead Code & Backwards-Compat Cruft)

> #1322 (LegacyFormId/LoadOrder re-export rot) verified **not regressed**
> (demoted to `pub(crate)`, zero production callers). No `#[deprecated]`, no
> `// removed:` breadcrumbs, no single-branch Cargo features.

#### TD8-001: Nine stale `#[allow(dead_code)]` on `CellLightingRes` fields — all now read
- **Severity**: LOW · **Dimension**: Dead Code · **Effort**: trivial
- **Location**: `byroredux/src/components.rs:269,283,287,291,295,299,303,306`
- **Status**: NEW.
- **Description**: The struct documents a staged-rollout policy: each field's allow is "removed in lockstep with the matching shader-side consumer landing." A consumer landed — `cell.lighting` (`commands.rs:1722-1757`) reads every field. The allows now defeat their own purpose (a genuinely-dead field added later would be masked).
- **Evidence**: Stripping all 9 allows + `cargo check -p byroredux` → zero dead-code warnings.
- **Suggested Fix**: Delete the 9 allow lines; keep the policy comment, noting the fields are now consumed by `cell.lighting`.

#### TD8-002: Stale `#[allow(dead_code)]` + comment on `RefrTextureOverlay::inner` — now used
- **Severity**: LOW · **Dimension**: Dead Code · **Effort**: trivial
- **Location**: `byroredux/src/cell_loader/refr.rs:64`
- **Status**: NEW (prior 2026-05-13 classification "future-tracked TD2-012" is now stale — the field is used at refr.rs:116,150,165).
- **Description**: Comment says "Not yet consumed by the spawn path," but `inner` is read/`&mut` in the slot-index-6 lookup. Removing the allow → no warning.
- **Suggested Fix**: Delete the allow and the "not yet consumed" comment.

#### TD8-003: `peek_path_filter` is genuinely dead — reserved for FO4-DIM6-02, never integrated
- **Severity**: LOW · **Dimension**: Dead Code · **Effort**: trivial
- **Location**: `crates/plugin/src/esm/records/mswp.rs:150-155`
- **Status**: Existing (tracked across 2026-05-13/2026-05-28 reports + `.claude/issues/1049`; still pending 2026-06-14).
- **Description**: `pub(crate) fn peek_path_filter` has zero callers repo-wide (incl. tests). The "Reserved for FO4-DIM6-02 stage-2" integration never landed. Stripping the allow → "function never used."
- **Suggested Fix**: Delete it (the FNAM read is trivial to re-add when FO4-DIM6-02 lands) or replace the bare comment with a concrete tracking-issue link. Recommend deletion given 3 audits pending.

#### TD8-004: `State::class_by_type_id` is write-only dead code
- **Severity**: LOW · **Dimension**: Dead Code · **Effort**: trivial
- **Location**: `crates/sfmaterial/src/reader.rs:187` (field), populated at :40,:61
- **Status**: Existing: #1324 (still present + dead). (The other half of #1324, `EguiPassConfig`, has been removed — zero matches.)
- **Description**: `class_by_type_id: HashMap<u32, usize>` is initialised + inserted-into but never queried (no `.get`/`.contains`). Only its sibling `class_by_name_offset` is read.
- **Suggested Fix**: Delete the field and the `.insert(...)` line.

---

### LOW — Dimension 9 (Test Hygiene)

> 246 of 247 `#[ignore]`s are legitimate Vulkan/game-data/env-gated baselines —
> not debt. #1320 (dump_prospector / translation_completeness zero-assertion)
> verified **not regressed** — both now carry real value assertions.
> `golden_frames.rs` intact; named must-not-regress tests
> (`bs_stream_header_not_read_for_off_spec_version`,
> `opaque_window_name_is_not_glass`) present and not ignored.

#### TD9-001: `dhat-heap` allocation-regression tests never run in CI
- **Severity**: LOW · **Dimension**: Test Hygiene · **Effort**: small
- **Location**: `crates/nif/tests/heap_allocation_bounds.rs:30`, `heap_allocation_bounds_geometry.rs:26`; CI `.github/workflows/ci.yml:30-31,58-59`
- **Status**: Existing: #1381 (OPEN).
- **Description**: Both tests are entirely behind `#![cfg(feature = "dhat-heap")]`. CI runs only `cargo test --workspace` (never `--features dhat-heap`); `dhat` appears nowhere in `.github/` or `docs/contributing.md`. The four allocation-hygiene pins (#832/#833/#831/#408) are verified only at audit-cadence, not CI-cadence — a re-introduced `or_insert(name.to_string())`-class allocation would pass CI silently.
- **Suggested Fix**: Add `cargo test -p byroredux-nif --features dhat-heap --test heap_allocation_bounds --test heap_allocation_bounds_geometry` to CI. Tracked by #1381 — no new issue needed.

---

## 6. Deferred

None this cycle. The two BLOCKED Vulkan splits (TD1-301/302) are reported as
LOW with the RenderDoc-capture gate noted rather than carried as separate
DEFERRED rows; the low-risk first step of TD1-302 (extract `context/types.rs`)
is ungated and actionable now.

---

## 7. Verification Ledger (recurring traps confirmed still-fixed)

| Trap | Prior issue(s) | Status today |
|------|----------------|--------------|
| GpuInstance = 112 B, GpuMaterial = 300 B prose | #1321/#1349 (CLOSED) | Correct (matches pins) |
| Vertex = 100 B / 19 f32 | — | Correct (pinned) |
| MAX_TOTAL_BONES size-history doc | #1451 (CLOSED) | Correct ("(current)" tagged) |
| `classify_pbr` framed as deleted (core/asset/static_meshes) | #1321/#1522 (CLOSED) | Correct (past-tense) — except new site TD3-002 |
| Bare BSVER decimal literals | #1336 (CLOSED) | No regression (named consts used) |
| DBG_* bit pins (all 13) | #1482 (CLOSED) | No regression (DBG_BITS catalog) |
| `read_zstring` single prod copy | #1318 (CLOSED) | No regression |
| 4 consolidated coord-flip sites | #1044/#1318 (CLOSED) | No regression (new sites = TD2-001, out of original scope) |
| dump_prospector / translation_completeness assertions | #1320 (CLOSED) | No regression |
| LegacyFormId/LoadOrder re-export | #1322 (CLOSED) | No regression (pub(crate)) |
| Path-validation gate | #1114 | OK — 776 refs / 24 files, 0 stale |

---

*Report generated by `/audit-tech-debt` (9 dimensions). Publish with:*
`/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`
