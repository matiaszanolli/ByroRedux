# REN-D11-2026-08-12-04: triangle.frag outMeshID comment still says "per-instance ID + 1"

## Description
Trailing comment still reads "per-instance ID + 1", the pre-`883f57cd` meaning for all draws. `gbuffer.rs` and `shader-pipeline.md` were both updated; the shader that actually *writes* the value was not — the single most load-bearing declaration of the two-meaning encoding. NEW (third site of the #2499/#2500 drift class)

## Location
`crates/renderer/shaders/triangle.frag` (`layout(location = 3) out uint outMeshID;`)

## Severity / Domain / Type
low / renderer / documentation

https://github.com/matiaszanolli/ByroRedux/issues/2759

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D11-2026-08-12-04).
