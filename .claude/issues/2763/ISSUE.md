# REN-D11-2026-08-12-07: water.vert stale "112-byte invariant" GpuInstance comment

## Description
Claims a "112-byte invariant" pinned by `gpu_instance_is_112_bytes_std430_compatible`; the struct body is **correct** (128 B, byte-identical to the other four mirrors) and the live test is `gpu_instance_is_128_bytes_std430_compatible` — no `_112_` symbol exists in the tree. `water.vert` is the only mirror still carrying the stale size claim, and historically the most likely to drift (#1498 had to add it to the name-drift guard list). NEW — three-way merge: Dim 11 `-07`, Dim 15 `REN-D15-06`, stale-run `REN-D11-03`. File once.

## Location
`crates/renderer/shaders/water.vert` (comment above `struct GpuInstance`)

## Severity / Domain / Type
low / renderer / documentation

https://github.com/matiaszanolli/ByroRedux/issues/2763

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D11-2026-08-12-07).
