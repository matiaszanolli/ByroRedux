# #4783: PERF-D5-2026-09-23-01: Transport emitters outside the 128 m froxel grid arm the grid-wide combustion stencil and its 78 s linger

**Severity**: MEDIUM
**Labels**: medium, performance, renderer, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23.md (PERF-D5-2026-09-23-01)

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**:
  - `byroredux/src/render/fog_volumes.rs:85` (frustum-only cull), `:105` (truncate after a near-to-far sort, no distance cull);
  - `crates/renderer/src/vulkan/volumetrics.rs:234-241` (`has_transport_emitter`), `:253-259` (`combustion_transport_active`), `:1155-1164` (`fog_reference[3] = simulation_dt`), `:1598-1615` (`requires_dispatch`).
- **Status**: NEW. This is a gap in #3131's gate, which keys on the list rather than the grid.
- **Description**:
  - `collect_fog_volumes` keeps any `FogVolume` whose bounding sphere passes the frustum test, at any distance.
  - `has_transport_emitter` then returns true for any Smoke/Flame/Explosion profile in that list. It is used both to arm `simulationDt > 0` and to push `combustion_active_until_seconds` 78 s ahead.
  - The shader's source injection only ever sees volumes inside the camera-centred 16³ × 16 m cluster cube. `build_fog_volume_clusters` drops the rest (`:559-574`), and `sampleLocalMedium` / `applyCombustionSources` are cluster-driven. So a flame beyond the grid far plane contributes nothing, yet it still switches the whole grid onto the RK2 path (PERF-D5-02).
  - `requires_dispatch` also returns true for `!fog_volumes.is_empty()`. In zero-extinction weather (`FogMedium::DISABLED`, `extinction_per_meter: 0.0`), one distant fire in view therefore dispatches the entire pass.
- **Evidence**:
  ```rust
  // fog_volumes.rs:85 — the only spatial test
  if frustum.contains_sphere(center, extents.length()) { out.push(gpu); }
  // volumetrics.rs:1609-1614
  if has_transport_emitter(fog_volumes) { self.combustion_active_until_seconds = now + COMBUSTION_HISTORY_LINGER_SECONDS; }
  has_global_medium || !fog_volumes.is_empty() || (self.history_valid && now <= self.combustion_active_until_seconds)
  ```
  Particle fires become Flame volumes by default (`fire_volume_from_particle`, `fog.rs:544`, `BYRO_FIRE_VOLUMES` default on), so every lit brazier, campfire or burning barrel in view qualifies.
- **Impact**: This is the full PERF-D5-02 cost (*est.* 0.3–0.6 ms at 1080p render; see that finding) in open exteriors whose only fires are distant, plus 78 s of the same cost after the last one leaves view. It is tier-invariant.
- **Related**: #3131 (closed; introduced the scene-level gate), PERF-D5-02.
- **Suggested Fix**: Filter once on the renderer side before `requires_dispatch` / `dispatch`. Drop volumes with `distance(center, camera) − radius > far_distance_world()`, reusing the same sphere test `build_fog_volume_clusters` already does, and feed the filtered slice to `has_transport_emitter`, `requires_dispatch` and the cluster build. Pin it with a unit test: an off-grid Flame must leave `combustion_transport_active` false.

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
