**Severity**: low (INFO-tier)
**Dimension**: Denoiser/Composite — cross-cutting doc (renderer audit 2026-07-14, DIM8)
**Location**: `docs/engine/shader-pipeline.md`, Shader Files table, `composite.frag` row (line ~25)
**Status**: NEW (CONFIRMED against HEAD)

## Description
The `composite.frag` row lists "direct + SVGF-denoised indirect, ACES tone-map, bloom add, volumetric froxel sample, underwater FX" but omits the two caustic accumulators (`causticTex` + `waterCausticTex`) summed into the direct-light term — a real composite responsibility.

## Evidence
- `shader-pipeline.md:25` — the row text, no caustic mention.
- `composite.frag` — `vec3 combined = direct + indirect * albedo + caustic;` where `caustic = albedo * causticLum` derived from bindings 5 (`causticTex`) + 8 (`waterCausticTex`).

## Impact
Documentation incompleteness only. `shader-pipeline.md` is the designated authoritative reference for the pipeline.

## Related
#1915 (sibling `shader-pipeline.md` descriptor/flag table drift).

## Suggested Fix
Append "+ dual caustic accumulator (glass/water)" to the `composite.frag` row.

## Completeness Checks
- [ ] **SIBLING**: While editing the row, confirm the volumetric/bloom/underwater terms listed still match the live `composite.frag` reassembly order.
- [ ] **TESTS**: N/A (doc-only).
