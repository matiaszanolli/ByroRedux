# PERF-D2-2026-09-11-01: `no_sorter` occupies sort-key slot 3 on the opaque and additive branches, partitioning two populations it has no ordering meaning for

Labels: low,performance,renderer,bug

**Description**: Slot 3 of the draw sort key is load-bearing only for the true-alpha-over branch (separating depth-sorted from state-clustered-opt-out draws). The opaque/additive branches also write `cmd.no_sorter as u8` into the same slot "for free" — but it sits above `render_layer`/`two_sided`/`pack_depth_state`/`mesh_handle`, so any opaque/additive population containing one `NoSorter` draw is cut into two blocks, doubling the layer/cull/depth-state ladder traversal. `no_sorter` is reachable on opaque draws: an alpha-tested `NiAlphaProperty` with bit 13 set produces an *opaque* `DrawCommand` carrying the marker (confirmed via #3797's own landing-commit census: 74/76 Oblivion meshes, concentrated in Ayleid ruins/shipwrecks/clutter; 0 on FO3/FNV).

**Evidence**:
`byroredux/src/render/mod.rs:846-849` (opaque), `:755-758` (additive) both write `cmd.no_sorter as u8` into the shared slot-3 position.

**Impact**: Does not split same-mesh instanced runs (the flag derives from the shape's own property, uniform per mesh) — only duplicates the state ladder and adds a `group_state` boundary (one extra indirect draw call) per affected cell. Small; needs an Oblivion Ayleid-ruin capture to quantify precisely.

**Related**: #3797 (the census this reasons from).

**Suggested Fix**: Write `0u8` into slot 3 on the opaque and additive branches; correct the doc block claiming this is "harmless".



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
