# REN-D3-2026-10-14-01: Fog-colour decode (`6eb1f378a`) left stale "authored/linear fog colour" wording in docs and shader comments

**Labels**: low,renderer,documentation,doc-rot
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D3-2026-10-14-01`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/shaders/include/bindings.glsl` (`vec4 fog; // xyz = fog color (RGB)`), `crates/renderer/shaders/cluster_cull.comp` comment, `docs/engine/shader-pipeline.md` GpuCamera row, `pack_fog_tint` docs, `docs/engine/procedural-volumetric-fog.md:97`

## Description
Fog colour reaching the renderer is now sRGB-decoded linear radiance (one decode at the `render/mod.rs` boundary; `CellLightingRes` keeps the authored value). The docs still say "fog color (RGB 0-1)" / "authored". No shader reads `camera.fog`; live consumers are composite `fog_color` and volumetric `fog_tint`. The decode matches the project's one fog-colour exception to "no sRGB decode". The commit also bundles an unrelated `.claude/issues/5155/ISSUE.md` and carries no tracking issue. Merges REN-D3-2026-10-14-01 and REN-D8-2026-10-14-02.

## Evidence
`bindings.glsl:309`: `vec4 fog; // xyz = fog color (RGB), w = fog enabled (1.0)`.

## Impact
Documentation drift only. Note: the `6eb1f378a` measurement is interior-only (FNV Wrangler); exterior behaviour (beyond-grid `authoredLuma*4` clamp, terrain-to-sky seam) is unverified.

## Suggested Fix
Update the wording and state the decode convention once in `shader-pipeline.md`.
