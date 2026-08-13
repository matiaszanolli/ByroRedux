# REN-D15-03: water.frag uv01 guard is inclusive on upper edge (off-by-one)

## Description
The screen-space guard is inclusive on the upper edge (`lessThanEqual(uv01, vec2(1.0))`), so at `uv01.x == 1.0` exactly, `pixel.x == screen.x` — one past the last valid texel. Benign (Vulkan discards out-of-range image writes) but it is the same conversion that runs wholesale against the 1×1 `placeholder_caustic_sink` fallback, relying on that robustness rule. `caustic_splat.comp` rejects explicitly against `size`.

## Location
`crates/renderer/shaders/water.frag` (the `uv01` guard and `ivec2 pixel` conversion)

## Severity / Domain / Type
low / renderer / bug

https://github.com/matiaszanolli/ByroRedux/issues/2784

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D15-03).
