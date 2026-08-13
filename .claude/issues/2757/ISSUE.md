# REN-D11-2026-08-12-03: helpers.rs comment pins stale triangle.frag:1532 anchor for bit-31 flag

## Description
Comment pins `triangle.frag:1532` for the bit-31 flag; that line is inside the glass Fresnel block, ~1000 lines from the real `outMeshID` write. A live example of exactly the rot the symbol-anchor rule exists to prevent — in the file the Dim-11 checklist names as its entry point, about the contract that just silently drifted (Cluster A/B).

## Location
`crates/renderer/src/vulkan/context/helpers.rs` (`create_render_pass`, attachment 3)

## Severity / Domain / Type
low / renderer / documentation

https://github.com/matiaszanolli/ByroRedux/issues/2757

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D11-2026-08-12-03).
