# AUDIT_RENDERER — 2026-05-01

**Auditor**: Claude Opus 4.7 (1M context)
**Baseline commit**: `feb5a56` (`Add issues and audit documentation for Fallout 3 compatibility`)
**Reference report**: `docs/audits/AUDIT_RENDERER_2026-04-27.md`
**Dimensions**: 15 (Sync · GPU Memory · Pipeline State · Render Pass · Command Recording · Shader Correctness · Resource Lifecycle · Acceleration Structures · RT Ray Queries · Denoiser & Composite · TAA · GPU Skinning · Caustics · Material Table (R1) · Sky/Weather/Exterior Lighting)
**Open-issue baseline**: 49 open issues at audit start (`/tmp/audit/renderer/issues.json`)
**Methodology**: Direct main-context delta audit. Sub-agent dispatches (3 parallel `renderer-specialist`s, then a re-dispatched batch) consistently stalled mid-investigation without writing the deliverable file — same pattern documented in 2026-04-27's methodology notes. Re-confirmed in this run: subagents exhausted their internal turn budget before the LAST writeback step, producing only a placeholder. Pivoted to direct audit anchored on the per-file commit churn since `7dc354a` (38 commits over 4 days, +2731/-1152 LoC across 33 renderer files).

---

## Executive Summary

**1 CRITICAL · 0 HIGH · 1 MEDIUM · 4 LOW · 1 INFO** — across 7 new findings.

The dominant change since 04-27 is the **R1 MaterialTable refactor** (commits `aa48d64` → `22f294a`, six phases landed today). The refactor is broadly sound — `GpuMaterial` is exactly 272 B, all-scalar (no `vec3` alignment hazard), Hash/Eq are byte-level over named-pad fields the producer always initialises, and the shader-side `struct GpuMaterial` matches the Rust layout in `triangle.vert`, `triangle.frag`, and `ui.vert` in lockstep.

**One CRITICAL finding (`R1-N1`) breaks the UI overlay path**: `ui.vert:73` reads `materials[inst.materialId].textureIndex` but the UI quad instance is appended at `draw.rs:961-970` with `..GpuInstance::default()`, leaving `materialId = 0`. After the bulk material upload, `materials[0]` is whichever scene material was first interned this frame — **not** the UI texture. The UI overlay therefore samples an arbitrary scene texture instead of the Ruffle-rendered overlay. The Rust-side comment on `GpuInstance.texture_index` (scene_buffer.rs:172-176) explicitly anticipates this failure mode ("the UI quad path appends an instance with a per-frame texture handle without going through the material table") — but Phase 5's `ui.vert` migration ignored that contract. `triangle.vert:157` still does the right thing (`fragTexIndex = inst.textureIndex`); only `ui.vert` deviates. One-line fix.

The MEDIUM finding (`R1-N2`) is the symmetric cleanup: `GpuInstance.texture_index` and `avg_albedo[rgb]` are kept on the per-instance struct as a deliberate exception to the Phase 6 collapse — for `ui.frag` (texture_index) and `caustic_splat.comp` (avg_albedo, descriptor-set isolation). Both retentions need a runtime invariant test or the next R1-style refactor will silently re-migrate them.

**Massive cleanup since 04-27**: at least **18 prior-audit findings closed** in 4 days (#573 partial, #645, #647, #648, #651, #654, #664, #665, #667, #679, #732, #737, #738, #739, #740, #743, #33, #541). The remaining 04-27 backlog is documented below; only 5 prior items are still load-bearing.

### What's new since 04-27

- **R1 MaterialTable refactor**, 6 phases, all closed today (`aa48d64` → `22f294a`). New module `crates/renderer/src/vulkan/material.rs` (457 LOC), 9 unit tests pinning byte layout + dedup invariants. `GpuInstance` collapsed from 400 B → 112 B (-72%). `MaterialBuffer` SSBO bound at set=1 binding=13 across all main-pass shaders.
- **M41.0 NPC spawn pipeline** (commits `ee6f87b`, `d5a9d03`, `61cc1ca`, etc.). Touches skinning end-to-end: NiSkinData field-order regression fixed, `MAX_TOTAL_BONES` bumped 4096 → 32768, FaceGen morphs (`.tri`/`.egt`/`.egm`) wired into NPC head spawn.
- **#732 closed** (`cb230ad`): explicit `flush_pending_destroys` drain in App shutdown sweep — the M40 Phase 1b exterior shutdown SIGSEGV is gone.
- **#647 / #648 / #573-partial / #645 / #654 / #740 / #739 / #743 / #737 / #738 / #667 / #651 / #665 / #679 / #664 / #33 / #541** all confirmed closed via per-file `git log` ranges below.

### What's still open from prior audits

| Prior ID | Site | Status |
|---|---|---|
| `SY-2` / `RP-N1` (#573) | `helpers.rs:163` | **STILL OPEN** — `BOTTOM_OF_PIPE` in dst_stage_mask of render-pass outgoing dep. Verified line 163 still emits the legacy term. |
| `SY-3` (#573) | `composite.rs:408` | **STILL OPEN** — same `BOTTOM_OF_PIPE` term. |
| `CMD-3` (#573) | `screenshot.rs:164` | **STILL OPEN** — same. |
| `LIFE-N2` | `swapchain.rs:202` | **STILL OPEN** — `pub unsafe fn destroy(&self, …)` should be `&mut self`. |
| `DEN-9` | `svgf.rs:792-854` | **STILL OPEN** — recreate_on_resize doesn't re-issue UNDEFINED→GENERAL. |
| `RT-11` / `RT-12` | `triangle.frag:1543, 1581` | **STILL OPEN** — reservoir shadow ray missing the `N_view` flip + tMin asymmetry. |
| `RT-14` | `triangle.frag:1635, 1610` | **STILL OPEN** — GI ray tMax=3000 vs fade window 4000-6000. |
| `SH-5` | `svgf_temporal.comp` | **STILL OPEN** — disocclusion gating by mesh-id only (no normal/depth). |
| `MEM-N2` / `MEM-N3` | `scene_buffer.rs:589`, `skin_compute.rs:274` | **STILL OPEN** — ray_budget BAR waste, skin-compute output `VERTEX_BUFFER` flag. |
| `PS-6` / `PS-7` / `PS-8` / `PS-9` | `pipeline.rs`, `helpers.rs:419` | **STILL OPEN** — static-vs-dynamic depth state drift hazards, cwd-relative pipeline_cache.bin. |
| `DEN-11` / `DEN-12` | `composite.frag:220, 208` | **STILL OPEN** — sky branch alpha-blend marker bit, wasted direct4 read on sky pixels. |
| `SH-13` | `composite.frag:113-150` | **STILL OPEN** — cloud UV mip-LOD oscillation (partial #730). |

### What's confirmed closed since 04-27

`AS-8-13` (#739, route drop_skinned_blas through pending queue), `AS-8-14` (#740, frame_counter advance in build_blas_batched), `MEM-N1` (#645, TLAS shrink), `LIFE-N1` (#732, shutdown flush sweep), `LIFE-N3` (#654, image-view destruction order), `LIFE-L1` (#665, Drop early-return), `RP-1` (#647, R16_UINT debug_assert), `RP-2` (#648, SVGF+TAA history-reset helper), `SH-6` (#651, bone-index clamp), `SH-12` (#667, caustic scale pin), `SH-14` (#737, SVGF nearest-tap fallback), `SH-15` (#738, caustic instance-index bounds), `DEN-10` (#743, exposure plumbing), `CMD-5` (#664, last-bound mesh cache), `R-10` (#33, teardown order helpers), `M33-09` (#541, SKY_LOWER plumbing), `#620` (BSEffect falloff cone reaches GPU), `#575` (SH-1, vertex SSBO float-reinterp guardrail).

---

## RT Pipeline Assessment

**BLAS / TLAS correctness**: solid. `AS-8-14` (cell-load eviction no-op) closed via `bd0db2f` — `frame_counter` now advances inside `build_blas_batched`, so streaming bursts no longer accumulate BLAS bytes past the budget. `AS-8-13` (skinned-BLAS deferred-destroy contract) closed via `f8a9719`. `MEM-N1` (TLAS shrink) closed via `6738c05`. No new findings in this dimension.

**Ray query safety**: ray sites in `triangle.frag` continue to bind the correct `topLevelAS`, gate on `rtEnabled = sceneFlags.x > 0.5` where applicable, and use `gl_RayFlagsTerminateOnFirstHitEXT` for shadows. `RT-11` (reservoir shadow ray missing `N_view` flip), `RT-12` (asymmetric tMin), and `RT-14` (GI tMax vs fade window) remain open from 04-27. The GI hit path now goes through `materials[hitInst.materialId]` (lines 380-388) — verified consistent with the primary path.

**Denoiser stability**: `SH-14` closed via `0b18cd8` (SVGF temporal nearest-tap fallback for sub-pixel silhouette miss). `DEN-10` closed via `4f705eb` (composite exposure through `depth_params.y` instead of compile-time const). `DEN-9` (SVGF resize UNDEFINED→GENERAL barrier) and `SH-5` (mesh-id-only disocclusion) remain open.

---

## Rasterization Assessment

**R1 MaterialTable refactor** is the headline rasterization change. Layout invariant pinned at `material.rs:339` (`gpu_material_size_is_272_bytes`). All-scalar contract enforced — no `vec3` fields. Hash/Eq use raw byte comparison via `as_bytes()`; named pad fields (`_pad_pbr`, `_pad_falloff`) are explicitly zeroed in `Default::default()` so byte-level Hash is deterministic across reachable values. `MaterialTable::intern` is O(1) amortised via `HashMap<GpuMaterial, u32>`, with 7 unit tests covering identity dedup, distinct-id assignment, texture-index discrimination, sub-epsilon float discrimination, and insertion-order stability.

**Critical regression (`R1-N1` below)**: the UI vertex shader was migrated to read texture-index from the materials buffer, but the UI quad path doesn't intern a UI material — it hard-defaults `material_id = 0` and stamps `texture_index` on the per-instance struct. The migrated `ui.vert:73` therefore reads `materials[0].textureIndex` (the first scene material's texture) instead of the UI texture. Triangle path unaffected.

**Pipeline state + render pass + command recording**: zero new findings beyond the stale-comment cleanup (`R1-N3`). Five `PS-*` and three `RP-*` items from 04-27 remain open as documented in the table above.

---

## Findings

### CRITICAL

#### R1-N1 — UI overlay reads `materials[0].textureIndex` but UI instance defaults `materialId = 0`

- **Severity**: CRITICAL
- **Dimension**: Material Table (R1)
- **Locations**:
  - `crates/renderer/shaders/ui.vert:73` — `fragTexIndex = materials[inst.materialId].textureIndex;`
  - `crates/renderer/src/vulkan/context/draw.rs:961-970` — UI instance pushed with `..GpuInstance::default()` (which sets `material_id = 0`)
  - `crates/renderer/src/vulkan/scene_buffer.rs:172-176` — Rust-side docstring explicitly anticipates this failure mode ("the UI quad path appends an instance with a per-frame texture handle without going through the material table; keeping it here costs 4 B per instance and avoids a UI-specific material-intern dance")
- **Status**: NEW (introduced by R1 Phase 5, commit `7a7c145`)
- **Description**: R1 Phase 5 migrated `ui.vert` to read texture index from the per-frame `MaterialBuffer` SSBO, but the UI quad never interns a material. At `draw.rs:963`, `gpu_instances.push(GpuInstance { texture_index: ui_tex, ..GpuInstance::default() })` leaves `material_id` at its default of `0`. The materials buffer at the time of this draw contains scene materials in insertion order — `materials[0]` is the **first scene draw's material**, not anything UI-related. `ui.vert:73` therefore writes that first scene material's `textureIndex` into `fragTexIndex`, and `ui.frag:15` samples `textures[fragTexIndex]` — yielding the wrong texture.
- **Evidence**:
  ```glsl
  // ui.vert:68-74
  void main() {
      gl_Position = vec4(inPosition.xy, 0.0, 1.0);
      fragUV = inUV;
      // R1 Phase 5 — read texture index from the material table.
      GpuInstance inst = instances[gl_InstanceIndex];
      fragTexIndex = materials[inst.materialId].textureIndex;
  }
  ```
  ```rust
  // draw.rs:961-970 — UI instance push
  let ui_instance_idx =
      if let (Some(ui_tex), Some(_)) = (ui_texture_handle, self.ui_quad_handle) {
          let idx = gpu_instances.len() as u32;
          gpu_instances.push(GpuInstance {
              texture_index: ui_tex,
              ..GpuInstance::default()  // material_id = 0
          });
          Some(idx)
      } else { None };
  ```
  ```rust
  // scene_buffer.rs:172-176 — design contract that R1 Phase 5 violated
  /// Diffuse / albedo bindless texture index. Held on the per-instance
  /// struct (not migrated to the material table) because the UI quad
  /// path appends an instance with a per-frame texture handle without
  /// going through the material table; keeping it here costs 4 B per
  /// instance and avoids a UI-specific material-intern dance.
  pub texture_index: u32, // 4 B, offset 64
  ```
  Note: `triangle.vert:157` still reads `inst.textureIndex` directly — only `ui.vert` was incorrectly migrated.
- **Impact**: Every UI overlay frame samples an arbitrary scene texture instead of the Ruffle-rendered overlay (or whichever texture handle the engine passed via `ui_texture_handle`). When the scene material count is zero (no draws — early menu / loading screen), `materials[0]` is undefined / out-of-bounds — driver-dependent; some Vulkan drivers return zero, some give garbage from prior allocations. Visible breakage on every frame that renders both scene + UI together (i.e. the entire normal play loop).
- **Suggested Fix**: Revert `ui.vert:73` to `fragTexIndex = inst.textureIndex;`. This matches `triangle.vert:157`, honours the `GpuInstance.texture_index` design contract documented in `scene_buffer.rs:172-176`, and avoids the alternative (more invasive) fix of interning a per-UI-frame material. Add a unit-test-equivalent shader-validation step that greps for `inst.textureIndex` in `ui.vert` to prevent re-regression.

### MEDIUM

#### R1-N2 — `GpuInstance.texture_index` and `avg_albedo[rgb]` retained as exceptions to Phase 6 collapse — no runtime invariant guards them

- **Severity**: MEDIUM
- **Dimension**: Material Table (R1)
- **Locations**:
  - `crates/renderer/src/vulkan/scene_buffer.rs:172-216` (struct fields + their docstrings explaining the retention)
  - `crates/renderer/shaders/caustic_splat.comp:153-155` (consumer of `avg_albedo`)
  - `crates/renderer/src/vulkan/scene_buffer.rs:1402-1418` (offset_of! tests)
- **Status**: NEW
- **Description**: Phase 6 collapsed ~30 per-material fields off `GpuInstance` (400 B → 112 B), but explicitly retained `texture_index` (UI exception, see R1-N1) and `avg_albedo[rgb]` (caustic-compute exception — `caustic_splat.comp` reads it from descriptor set 0 / binding 5 without a `MaterialBuffer` binding, and migrating the caustic compute pipeline was deferred). These retentions are documented in field docstrings but not enforced by any test. A future refactor sweep that re-migrates these fields without simultaneously fixing the consumer paths would silently break the UI (in the texture_index case) or the caustic compute (in the avg_albedo case) — exactly the failure mode that R1-N1 demonstrates is non-hypothetical.
- **Evidence**: The 04-27 audit's `MEM-N1` had a runtime invariant test (`scratch_should_shrink`); no equivalent exists for these per-instance retentions. The byte-offset tests at `scene_buffer.rs:1402-1418` pin the layout but say nothing about *why* these fields are still here.
- **Impact**: Silent regression risk on the next R1-style sweep. The R1-N1 finding above is the same class of bug, already exercised.
- **Suggested Fix**: Add two targeted invariant checks: (a) a static-string assertion in build that `ui.vert` contains `inst.textureIndex` (mirroring the existing `gpu_material_size_is_272_bytes` style), and (b) a comment-cross-link in `caustic_splat.comp` asserting `avg_albedo` is read from `instances[]` not `materials[]` until the caustic pipeline gets its own `MaterialBuffer` binding. Alternatively, file a `R1-cleanup` follow-up tracking issue to fully migrate both consumers (intern a UI material, add MaterialBuffer to caustic descriptor set 0) so the per-instance fields can drop on the next pass.

### LOW

#### R1-N3 — Stale comments in `triangle.frag` reference the dropped `inst.<field>` per-material paths

- **Severity**: LOW
- **Dimension**: Material Table (R1)
- **Locations**:
  - `crates/renderer/shaders/triangle.frag:592` — "use `mat.<field>` instead of `inst.<field>` for any per-material data" (correct guidance, but the comment goes on to reference the legacy fields as if they still existed)
  - `crates/renderer/shaders/triangle.frag:699-702` — "The per-instance `inst.roughness` slot is still populated by the CPU pipeline (Phase 6 drops them)" — Phase 6 already dropped them; this comment is now wrong
  - `crates/renderer/shaders/triangle.frag:914` — "modulation lerps from the upstream (`inst.roughness`)" — same; the field no longer exists on `GpuInstance`
  - `crates/renderer/shaders/triangle.frag:951` — "`inst.envMapIndex != 0u`" — `envMapIndex` lives on `materials[…]` now
- **Status**: NEW
- **Description**: Several comments in `triangle.frag` were written during Phase 4-5 transition and describe `inst.<field>` as "still populated" or "byte-equal to" `materials[…].<field>`. Phase 6 dropped those fields entirely; the comments now refer to non-existent struct members.
- **Impact**: Minor reader confusion; no functional issue. Future shader edits could be misled into trying to `inst.roughness` (which would fail to compile, so harm is bounded).
- **Suggested Fix**: Sweep the four sites and rewrite the comments to describe the current state. One PR, ~5 minutes.

#### R1-N4 — `GpuInstance` Phase-6 trim doesn't collapse `texture_index` + `_pad_id0`; layout has a 4 B pad slot at offset 92 that could absorb a useful field

- **Severity**: LOW
- **Dimension**: Material Table (R1)
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:177-202`
- **Status**: NEW
- **Description**: The Phase-6 layout is:
  ```
  offset 64: texture_index (u32) — UI exception
  offset 68-80: bone_offset, vertex_offset, index_offset, vertex_count (u32 × 4)
  offset 84: flags (u32)
  offset 88: material_id (u32)
  offset 92: _pad_id0 (f32) — explicit pad
  offset 96-104: avg_albedo_r/g/b (f32 × 3) — caustic exception
  offset 108: _pad_albedo (f32) — explicit pad
  ```
  Total 112 B. The pad at offset 92 exists because `avg_albedo` starts at a fresh vec4 boundary (96). A future field addition before `avg_albedo` would land at offset 92 free of cost.
- **Impact**: None today. Tracking item for whoever adds the next per-instance field — they should consume the existing pad rather than growing the struct.
- **Suggested Fix**: No action. Add a comment at the `_pad_id0` declaration noting "next per-instance u32 lands here for free."

#### R1-N5 — `MaterialTable::intern` HashMap<GpuMaterial, u32> stores the full 272 B material as the key — ~3× memory cost vs hashing once and storing the hash

- **Severity**: LOW
- **Dimension**: Material Table (R1)
- **Location**: `crates/renderer/src/vulkan/material.rs:275-313`
- **Status**: NEW
- **Description**: `MaterialTable.index: HashMap<GpuMaterial, u32>` stores 272 B per unique material. With `MAX_MATERIALS = 4096` (per `scene_buffer.rs:57`), the worst-case overhead is `272 × 4096 × 2 (HashMap load factor) ≈ 2.2 MB` of CPU heap **per frame** plus the 1.1 MB SSBO upload. Each HashMap entry also holds a separate copy of the material (the key) AND its hash. A `HashMap<u64, u32>` keyed on a precomputed `xxh3` of the material bytes would shrink the key surface 33× and trade an additional collision-check round trip for the deduplication cost.
- **Impact**: Today: trivial — cell loads are ~1k unique materials, ~270 KB cost. At scale (full Skyrim city load with 4k unique materials × 60 fps): the HashMap's per-frame `clear()` + `insert()` churn becomes a tens-of-µs CPU cost on hot loops. Not a correctness issue; a forward-looking polish item.
- **Suggested Fix**: Defer to a later optimization pass. If profiled and chosen, switch to `HashMap<u64, u32>` with `xxh3_64(material.as_bytes())` as the key, plus a collision-check `materials[id] == material` round-trip. Pre-#NNN tracking only.

#### CMD-N1 — `cmd_set_depth_bias(cmd, 0.0, 0.0, 0.0)` issued unconditionally on every draw before the first decal-flagged batch

- **Severity**: LOW
- **Dimension**: Command Recording
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` — re-flag of #51 ("Perf: unconditional cmd_set_depth_bias on every draw")
- **Status**: **Existing: #51** (open since pre-2026-04 audit cycle)
- **Description**: Issue #51 is the longest-open performance finding in the renderer; it wasn't re-listed in 04-27 but remains a per-frame command-stream waste. `cmd_set_depth_bias` is part of the dynamic-state list for the opaque pipeline; the CPU emits the call on every draw whether or not the next pipeline actually consumes the bias.
- **Suggested Fix**: Track depth-bias state per-frame and only emit `cmd_set_depth_bias` when the value changes. Single-batch optimisation; ~20 lines.

### INFO

#### R1-N6 — `GpuMaterial` has 17 vec4 slots; field-order rearrangement could collapse one slot if `material_id` column shifts

- **Severity**: INFO
- **Dimension**: Material Table (R1)
- **Location**: `crates/renderer/src/vulkan/material.rs:46-148`
- **Status**: NEW (acknowledged design tradeoff)
- **Description**: The current 17-vec4 layout was chosen to keep Phase 4–5 mechanical (rename `instance.foo` to `materials[material_id].foo` with no offset shuffle). The downside: vec4 #6 has `env_mask_index, alpha_test_func, material_kind, material_alpha` — three u32 + one f32 mixed in one slot. Slots #10/11 split `skin_tint_a` from `skin_tint_r/g/b`, requiring shader-side awkward reads. A clean-slate redesign could likely fit the same fields into 14-15 vec4 slots (~10% memory saving on a 1.1 MB SSBO). Not a bug; just noting for future material-system iteration.
- **Suggested Fix**: No action. Documented for future rework.

---

## Prioritized Fix Order

1. **`R1-N1` (CRITICAL)** — UI overlay texture regression. One-line fix at `ui.vert:73`. **Do this first** — every frame with UI active is rendering wrong pixels.
2. **`R1-N3` (LOW)** — sweep stale comments in `triangle.frag` referencing dropped `inst.<field>` paths. ~5 min.
3. **`R1-N2` (MEDIUM)** — add invariant guards on the `GpuInstance.texture_index` and `avg_albedo` retentions so the next R1-style sweep can't repeat the R1-N1 mistake. ~30 min.
4. **#573 PR bundle (3 sites)** — `helpers.rs:163`, `composite.rs:408`, `screenshot.rs:164` BOTTOM_OF_PIPE in dst_stage_mask. Validation-layer cleanup, ready to merge.
5. **`LIFE-N2` (LOW)** — `swapchain.rs:202` change `&self` to `&mut self` for `destroy()`. Open from 04-27.
6. **`DEN-9` (MEDIUM)** — SVGF resize UNDEFINED→GENERAL barrier. Open from 04-27.
7. **`RT-11..14` bundle** — three small RT shadow / GI ray ergonomics items. Open from 04-27.
8. **`R1-N4` / `R1-N5` / `R1-N6`** — defer.

---

## Out-of-scope (Filed Separately)

- **#729 / #730 / #731** — exterior FNV WastelandNV visual issues; CPU-side WTHR slot audit (not GLSL).
- **M41 NPC skinning regression** — recent body-skinning fixes (`87d3fc0`, `8ec6a69`) appear sound at the field-order layer; skinned BLAS rebuild policy (#679, `9d6a8b1`) is in place. Not deeply audited this pass; defer to a M41-focused audit.
- **Sky / weather (Dim 15)** — `#541` (M33-09 SKY_LOWER) closed; `#539` (parse_wthr / parse_clmt GameKind gate) and `weather_system` ↔ TOD palette interaction are unchanged from 04-27. Not re-audited this pass.

---

## Methodology Notes

- **Sub-agent dispatch failure recurred**: Phase 2 launched 3 `renderer-specialist` agents in parallel for Dims 1-3, all of which completed without writing their dim_N.md output files. A re-dispatched batch with explicit "MANDATORY FIRST STEP: Write placeholder file" prompts also stalled mid-investigation — agents wrote the placeholder but never overwrote it with findings (turn budget exhausted before the LAST step). Same pattern documented in 2026-04-27's methodology notes; the recommended workaround there was "general-purpose agents with file-first prompting + ruthless brevity caps." This audit pivoted further: direct main-context investigation anchored on the per-file `git log 7dc354a..HEAD` churn ranges.
- **Dedup baseline**: 49 open issues at audit start (`/tmp/audit/renderer/issues.json`), plus 16 prior renderer audit reports in `docs/audits/AUDIT_RENDERER_*.md` (most recent 2026-04-27, 51 KB / 28 findings).
- **Coverage trade-off**: this audit prioritised depth on the R1 refactor (the largest 4-day delta) over breadth across all 15 dimensions. Dims 11 (TAA), 12 (Skinning), 13 (Caustics), 15 (Sky) received only delta sweeps — `git log` confirmed each had a small, focused fix landed and no fresh hazards were introduced. The R1 finding (`R1-N1`) was the only NEW correctness issue surfaced.
- **Verification discipline**: every finding's premise was re-confirmed by reading the current code at the cited lines before report-out. The R1-N1 chain was independently verified at three sites (ui.vert:73 GLSL read, draw.rs:961-970 UI instance push, scene_buffer.rs:172-176 design-contract docstring) before promotion to CRITICAL.
