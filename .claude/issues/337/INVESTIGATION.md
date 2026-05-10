# Investigation — D4-NEW-01 (NiStencilProperty)

## Audit premise vs current code

Audit: `>95%` of `NiStencilProperty` usage is for two-sided rendering, which works
today via `is_two_sided()` in the walker. The remaining ~5% (stencil-masked decals,
portals, shadow volumes) silently drops the stencil parameters at the importer
boundary — the parser captures all 7 fields, the walker only consumes `draw_mode`
via `is_two_sided()`.

**Verified at HEAD** (commit `cfc89af`):
- Parser: `crates/nif/src/blocks/properties.rs:1471-1587` captures
  `stencil_enabled`, `stencil_function`, `stencil_ref`, `stencil_mask`,
  `fail_action`, `z_fail_action`, `pass_action`, `draw_mode` — both Oblivion
  expanded format and FO3+ packed-flags format.
- Walker: `crates/nif/src/import/material/walker.rs:738-745` only reads
  `is_two_sided()` and promotes `info.two_sided`.
- Renderer: every pipeline-create site (`pipeline.rs:301, 462, 600`) hardcodes
  `stencil_test_enable(false)`.

## Renderer-side dependency: depth format

`find_depth_format` (`crates/renderer/src/vulkan/context/helpers.rs:10-35`) prefers
`D32_SFLOAT` (no stencil bits) over `D32_SFLOAT_S8_UINT` / `D24_UNORM_S8_UINT`.
Current depth image on most desktop hardware therefore has zero stencil storage.

A real stencil-pipeline-variant landing would need to:
1. Promote one of the stencil-capable formats above pure-depth in the candidate
   list (gated on the workload actually demanding it — pure D32 is the better
   precision pick when no consumer needs stencil).
2. Add per-`MaterialKind` pipeline variants honouring the captured stencil ops.
3. Add `vkCmdSetStencilReference` / `vkCmdSetStencilCompareMask` /
   `vkCmdSetStencilWriteMask` calls per draw if the variants use dynamic state.

That's a renderer-architecture change, not a parser fix. Per
`feedback_speculative_vulkan_fixes.md` it stays out of scope until a
visible-content driver lands (Oblivion stencil-portal cell or stencil-shadow
volume mesh that actually misbehaves on the bench).

## Project precedent: capture-and-defer

`MaterialInfo.wireframe` (#869) and `MaterialInfo.flat_shading` (#869) follow the
exact shape this fix needs:
- Importer captures the bit.
- `MaterialInfo` carries the field with a docstring naming the renderer-side
  deferral and pointing at the issue.
- Renderer-side consumption is "future work" without a separate dispatch path.

`MaterialInfo.effect_shader: Option<BsEffectShaderData>` and
`MaterialInfo.no_lighting_falloff: Option<NoLightingFalloff>` follow the
same pattern at structure-shaped granularity — collapse N related fields into
one `Option<...>` so callers branch once.

## Plan

Minimum-surface fix matching project precedent:

1. Add `pub struct StencilState { ... }` next to `NoLightingFalloff` /
   `BsEffectShaderData` in `import/material/mod.rs` carrying the seven non-
   `draw_mode` fields. `draw_mode` stays consumed via `is_two_sided()` — that
   path works today, no need to duplicate.
2. Add `pub stencil_state: Option<StencilState>` to `MaterialInfo` with a
   docstring naming the renderer-side deferral.
3. Update the walker (`walker.rs:738-745`) to capture both: the `is_two_sided()`
   promotion stays, plus a parallel capture of the full state into
   `info.stencil_state`. Two-sided-only stencil properties (the 95% case) still
   set `stencil_state` so the field reflects what the NIF authored — the renderer
   gates on `stencil_enabled` to skip the no-op state.
4. Cross-reference comment at `pipeline.rs:301, 462, 600` (the three hardcoded
   `stencil_test_enable(false)` sites) so the future renderer-side fix lands at
   one grep target.
5. Regression test on the importer: synthetic NIF with a stencil-enabled
   `NiStencilProperty` round-trips into `MaterialInfo.stencil_state`.

No new dependencies, no Vulkan changes, no shader changes. Closes the silent-drop
half of the audit; the renderer-side variant stays deferred behind the
"visible-content driver lands" gate per precedent.

## Files touched

- `crates/nif/src/import/material/mod.rs` — add `StencilState` struct, field on
  `MaterialInfo`, default
- `crates/nif/src/import/material/walker.rs` — capture full state (not just
  two-sided)
- `crates/renderer/src/vulkan/pipeline.rs` — three cross-reference comments
- `crates/nif/src/import/material/` — new test module (or add to existing
  walker tests)

5 files. Within the `>5 file` scope-check threshold (counting the test as
content-of-existing-file), no pause needed.
