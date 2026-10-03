# #5216 — REN-D8-2026-10-03-03: two of the #4784 guards cannot fail on the regressions they name

**Labels**: low,renderer,test-gap,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs`, `mod transport_occupancy_tests`: `build_marks`, `occupancy_marks_reset_between_builds`, `transport_occupancy_gate_is_wired_into_the_inject_pass`
- **Status**: NEW
- **Description**:
  1. `occupancy_marks_reset_between_builds` claims to pin "a fresh build must not inherit the previous frame's marks". However, `build_marks` allocates a new zeroed `Box<[u32; FOG_VOLUME_CLUSTER_COUNT]>` on every call, so the second build always starts from zero. The test also never reaches production's empty-volume arm, `self.fog_cluster_occupancy.fill(0)` in `dispatch`, because that arm does not call `build_fog_volume_clusters`. It stays green if either `occupancy.fill(0)` (in `build_fog_volume_clusters`) or the empty-branch `fill(0)` is deleted.
  2. The call-site check in `transport_occupancy_gate_is_wired_into_the_inject_pass` falls back to `shader.find("transportOccupied,")`. That needle also matches the parameter declaration `bool transportOccupied,` in `transportCombustion`'s signature. If the call site regressed to passing `true`, the gate would be bypassed and the assertion would still pass.
- **Evidence**: `fn build_marks(volumes: &[GpuFogVolume]) -> Box<[u32; FOG_VOLUME_CLUSTER_COUNT]> { … let mut occupancy = Box::new([0u32; FOG_VOLUME_CLUSTER_COUNT]); …`. The `.or_else(|| shader.find("transportOccupied,"))` fallback. `volumetrics_inject.comp`'s `bool transportOccupied,` parameter.
- **Impact**: Test gap only. A missing reset fails conservatively: stale marks keep transport running wherever fire ever burned, which costs performance but not correctness. A literal `true` at the call site silently undoes the #4784 performance fix.
- **Related**: #4784.
- **Suggested Fix**: Reuse one occupancy buffer across both `build_fog_volume_clusters` calls with a non-empty second volume list, and separately assert the `dispatch` empty branch's `fill(0)` via `production_text`. Drop the fallback needle, or match the call's argument list exactly.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
