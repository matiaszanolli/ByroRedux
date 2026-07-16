# Renderer Audit — 2026-07-15 (Dimension 15: Water (M38) + water-side caustics)

Scope: `--focus 15` — single-dimension run of `/audit-renderer`, `--depth deep`.
Audited against `docs/engine/shader-pipeline.md` and `docs/engine/memory-budget.md`
(both confirmed current — no doc drift found in the water scope).

## Executive Summary

Water rendering + water-side caustics are in good shape. Every regression
guard for this dimension is intact: sun-direction re-upload, the
`WaterCausticAccum` per-FIF lifecycle and barrier sequencing, the
`water_caustic.rs`/`water.rs` module boundary, the live `imageAtomicAdd`
write, and the composite direct-only consumption rule (no double-count
against SVGF-denoised indirect). Submersion hysteresis is solid and
unit-tested; shading uses the correct water Fresnel (0.02, not glass's 1.5
IOR) and IOR (1.33) with non-black/non-magenta ray-miss fallbacks in both
reflection and refraction. No CRITICAL or HIGH findings, and nothing
resembling the documented GPU-TDR "crash near water" confound from an
earlier session (that was root-caused to an unrelated RT overhaul, not
water code — correctly not rediscovered here).

Two genuine findings survived scrutiny, both visual-quality rather than
correctness/crash class:

- **REN-D15-01 (MEDIUM)** — the #1502 procedural-noise precision bound is
  comment-only, and the comment's own premise ("procedural is never a
  default path") is factually false against current code.
- **REN-D15-02 (LOW)** — a stale/inaccurate comment claims water gets
  depth-bias z-fight protection via `RenderLayer::Decal`; the water
  pipeline actually disables depth bias entirely.

No bench-of-record comparison — this was a static/functional code trace,
not a live rendering run.

## Findings

### REN-D15-01: Procedural-noise precision guard is comment-only, and its "never a default path" premise is false
- **Severity**: MEDIUM
- **Dimension**: Water
- **Location**: `crates/renderer/shaders/water.frag` (`sampleScrollingNormal`, procedural branch); `byroredux/src/cell_loader/water.rs` (`spawn_water_plane`); `crates/core/src/ecs/components/water.rs` (`WaterMaterial::default`); `byroredux/src/env_translate.rs` (`resolve_water_material`)
- **Status**: NEW (a regression-class gap stemming from #1502 — the documented guard never became an *enforced* guard)
- **Description**: `sampleScrollingNormal`'s procedural branch (taken when `normalMapIndex == 0xFFFFFFFFu`) carries a comment noting it feeds absolute-world-XZ coordinates into `hash21`, which loses precision and visibly bands past a documented threshold — with the caveat "if procedural foam/noise ever becomes a default path, feed the hash render-origin-relative coordinates." That condition is **already met**: `WaterMaterial::default()` sets `normal_map_index: u32::MAX`, and `resolve_water_material` leaves the material at that default whenever a cell has no `XCWT` water-type ref (true for every FNV/FO3/Oblivion cell — those games don't populate the Skyrim-style XCWT field) or when the referenced WATR record has an empty `texture_path` (confirmed by the existing `resolve_water_material_transfers_reflection_color` test's own `LavaPool01` fixture, which sets `texture_path: String::new()`). The procedural path is therefore the **default** for a large class of real content, not an edge case.
- **Evidence**:
  ```rust
  // water.rs (components) — WaterMaterial::default()
  normal_map_index: u32::MAX,
  ```
  ```rust
  // env_translate.rs::resolve_water_material — only overrides on a resolvable XCWT + non-empty texture_path
  if let Some(form) = xcwt_form {
      if let Some(rec) = waters.get(&form) {
          // ... normal_path = Some(..) only when !rec.texture_path.is_empty()
      }
  }
  ```
  ```glsl
  // water.frag
  if (normalMapIndex == 0xFFFFFFFFu) {
      // PRECISION BOUND (#1502): `uvBase` here is absolute world XZ ...
      // ... if procedural foam/noise ever becomes a default path, feed the hash
      // render-origin-relative coordinates [not done]
      vec2 uv = uvBase * scale + scroll * time;   // uvBase = vWorldPos.xz, ABSOLUTE
      ...
  }
  ```
- **Impact**: Visual-only — banded/quantized wave normals on distant exterior water lacking a bound normal map. Reachable at real-world magnitudes: Skyrim's Tamriel worldspace extends to roughly ±233k units, FNV's Mojave far cells reach `grid * 4096`, both well past the ~176k-unit band-onset the comment itself documents. No crash, no gameplay impact, no CPU-side corruption.
- **Related**: #1502 (origin of the precision comment this finding shows never got wired to an actual coordinate rebase).
- **Suggested Fix**: Apply the fix the comment already prescribes — rebase the procedural branch's input to `vWorldPos.xz - renderOrigin.xz` (or an equivalent `fract`-reduced lattice) before hashing, matching the render-origin-relative convention used elsewhere in the raster path. Update the comment's "never a default path" claim once fixed (or now, since it's currently inaccurate either way).

### REN-D15-02: Water's `RenderLayer::Decal` z-fight comment claims a depth-bias protection the pipeline doesn't apply
- **Severity**: LOW
- **Dimension**: Water
- **Location**: `byroredux/src/cell_loader/water.rs` (`spawn_water_plane`, `RenderLayer::Decal` insert comment); `crates/renderer/src/vulkan/water.rs` (`build_pipeline` rasterizer state, `WATER_PIPELINE_DYNAMIC_STATES`); `crates/renderer/src/vulkan/context/draw.rs` (dedicated water draw loop)
- **Status**: NEW
- **Description**: `spawn_water_plane` tags the spawned plane `RenderLayer::Decal` with a comment claiming this "pushes water onto a slightly biased depth ladder so it stays above coincident architectural geometry ... without z-fighting." In the current pipeline this is not true: `water.rs`'s `build_pipeline` sets `.depth_bias_enable(false)` in the rasterizer state, `WATER_PIPELINE_DYNAMIC_STATES` does not include `DEPTH_BIAS`, and the dedicated water draw loop in `draw.rs` sets only depth-test/write/compare-op and cull-mode dynamic state per draw — never `cmd_set_depth_bias`. `RenderLayer::Decal` still does real work for water (it places water late in the sorted draw order via the sort key), but the depth-bias half of the stated rationale isn't realized by any code path.
- **Evidence**:
  ```rust
  // water.rs::build_pipeline
  .depth_bias_enable(false)
  // WATER_PIPELINE_DYNAMIC_STATES = [VIEWPORT, SCISSOR, DEPTH_TEST_ENABLE,
  //                                   DEPTH_WRITE_ENABLE, DEPTH_COMPARE_OP, CULL_MODE]
  //   (no DEPTH_BIAS)
  ```
  Water is also routed around the main opaque-decal batch loop (`skip_batch = !draw_cmd.in_raster || draw_cmd.is_water`), i.e. it never goes through wherever Decal depth-bias would normally apply to other decal geometry.
- **Impact**: Practical risk is low — water never writes depth (`depth_write` off), so the two surfaces don't fight for the depth buffer the way two opaque decals would; residual risk is at most a thin comparison-order band exactly at the shoreline where the bed mesh crosses the water plane, not full-surface flicker. This is a documentation-accuracy finding more than a functional one.
- **Related**: REN-D15-01 (same file, same general "shoreline surface quality" checklist item).
- **Suggested Fix**: Prefer correcting the comment to state Decal is used here purely for draw-order placement, not depth bias, since no shoreline speckle has been observed. Only add real `DEPTH_BIAS` dynamic state + a `cmd_set_depth_bias` call to the water draw loop if a RenderDoc capture actually shows shoreline z-fighting.

## Regression Guards Verified Intact (not re-proposed)

- **Spawn correctness**: water plane spawns at the authored `XCLW` height with the correct interior/exterior half-extent and WATR-derived material params; `water.vert` performs no vertex displacement on the flat quad, so there is no NaN-producing displacement path.
- **Shading model**: `fresnel_f0` defaults to 0.02 (correctly distinct from glass's IOR-1.5 Fresnel base), WATR-authored fresnel clamped to `[0.001, 0.20]`; IOR defaults to 1.33; reflection-ray miss falls back to `skyTint`, refraction-ray miss falls back to `push.deep.rgb` — neither path produces black or magenta; all rays gated behind `sceneFlags.x < 0.5`.
- **Submersion**: `resolve_head_submerged` uses a symmetric hysteresis band (`WATERLINE_HYSTERESIS = 4.0`), unit-tested against strobing at the waterline boundary.
- **Cell-unload cleanliness**: water has no BLAS (`build_blas = false`), so nothing can leak into a rebuilt TLAS; the normal-map texture handle's refcount is released via `NormalMapHandle` at unload (#1338).
- **Shadow/culling**: no BLAS means water can never appear in the TLAS and therefore casts no opaque shadow; two-sidedness is dynamic `CULL_MODE` (baseline state `NONE`), not a static pipeline flag.
- **Sort/batching**: `draw_sort_key` doesn't special-case `is_water` — water sorts through the same opaque-decal ordering as everything else, and `reemit_water_planes`'s post-sort slot assumption is guarded by a debug-assert-backed test (`water_commands_match_draw_slots`). Water is skipped out of the main triangle draw path entirely (push-constant driven, no material-table read), so there is no dedup-collapse risk with glass's `GpuMaterial` entry.
- **Water-side caustic sun direction**: rebuilt from `sky_params` and re-uploaded every frame via `upload_camera` — not stale-from-init.
- **`WaterCausticAccum` lifecycle**: per-frame-in-flight `R32_UINT` image; correct `UNDEFINED → GENERAL` init and per-frame `GENERAL → TRANSFER_DST(clear) → GENERAL` transitions; reverse-order `destroy_slot`, torn down inside the allocator-guarded `Drop` before the allocator itself drops; resize correctly re-wires the image views.
- **Module boundary**: the accumulator's image lifecycle lives entirely in `water_caustic.rs`; `water.rs` owns only the Set-2 descriptor layout/pool/sets for it — the intended separation holds.
- **Live write path**: `water.frag` reaches a real `imageAtomicAdd(waterCausticAccum, ...)` call on the sun-visible → refract → floor-hit path (not a dead branch); the floor projection is correctly rebased by `renderOrigin`.
- **Composite direct-only rule**: the water caustic texture is summed with the glass caustic texture and added into composite's `combined` direct term — never into the SVGF `indirectTex` — so there's no double-count through temporal accumulation.

## Doc Consistency

`docs/engine/shader-pipeline.md` (Set 2 Binding 0 = `STORAGE_IMAGE(R32_UINT)`, `water.frag`'s stated roles) and `docs/engine/memory-budget.md` (water caustic accumulator sizing, ×2 FIF) both match what was found in code. No doc drift identified in this dimension's scope.

---
Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_2026-07-15_DIM15.md`
