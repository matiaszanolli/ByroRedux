# REN-D12-2026-08-12-02: order_dependent_glass fragments opaque MultiLayerParallax batches unnecessarily

## Description
`order_dependent_glass` is computed for **every** batch from `is_refractive_glass`, which accepts opaque MultiLayerParallax; `group_state` carries it unconditionally, so an *opaque* MLP batch gets a distinct merge key and fragments an otherwise-homogeneous run into three indirect groups — where the split it protects can never apply (`needs_two_sided_blend_split` requires `is_blend`). Draw-call-count noise only; zero on games with no such content.

## Location
`crates/renderer/src/vulkan/context/draw.rs` (`group_state`, `needs_two_sided_blend_split`)

## Severity / Domain / Type
low / renderer / bug

https://github.com/matiaszanolli/ByroRedux/issues/2764

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D12-2026-08-12-02).
