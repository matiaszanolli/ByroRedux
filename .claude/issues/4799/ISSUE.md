# #4799: PERF-D2-2026-09-23b-01: The sky-aperture dust floor scans the whole draw list every interior frame to find architecture glass

**Severity**: LOW
**Labels**: low, performance, renderer, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D2-2026-09-23b-01)

- **Severity**: LOW
- **Dimension**: Draw & Instancing (per-frame CPU in `build_render_data`). Merges PERF-D1-2026-09-23b-04.
- **Location**: `byroredux/src/render/mod.rs:1306-1319` (`0572bfd5a`)
- **Status**: NEW
- **Description**:
  - `draw_commands.iter().any(|d| d.material_kind == MATERIAL_KIND_GLASS && d.render_layer == Architecture)` walks the whole post-sort list at a ~480 B stride.
  - It runs on every interior frame without a fog override, sky exposure or light-shaft volume.
  - The answer is effectively cell-static: glass is never TLAS-excluded.
  - `collect_static_mesh_draws` already reads both fields.
- **Impact**: *est.* 15–60 µs/frame at MedTek (13,545 commands), 1–15 µs in typical interiors, 0 on exteriors.
- **Suggested Fix**: Return `saw_architecture_glass` from `collect_static_mesh_draws`, or cache the flag per cell.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
