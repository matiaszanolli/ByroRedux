# #3611 — REN-2026-08-30-D16-05: the volumetric far plane has three unpinned copies of its default

**Labels**: `low,renderer,test-gap,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3611 --json state`.

---

- **Severity**: Low
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/upscaling.rs:137` (`grid_far_meters: 128`), `crates/renderer/src/vulkan/volumetrics.rs:268` (`DEFAULT_GRID_FAR_METERS: f32 = 128.0`), `crates/renderer/src/shader_constants_data.rs:354` (`VOLUME_FAR: f32 = 8_960.0`)
- **Status**: OPEN — new (regression guard)
- **Description**: The same default — 128 m — is written independently in
  three places. `VOLUME_FAR = 8_960.0` is the same value pre-multiplied by
  `BETHESDA_UNITS_PER_METER = 70.0` and its own comment calls it *"the
  canonical default for diagnostics and shader-contract tests"*, but nothing
  asserts `VOLUME_FAR == DEFAULT_GRID_FAR_METERS * 70.0 ==
  VolumetricsConfig::default().grid_far_meters as f32 * 70.0`. `grep` finds no
  test relating any pair.
- **Evidence**:
  - `crates/renderer/src/vulkan/volumetrics.rs:268`–`269` — `DEFAULT_GRID_FAR_METERS: f32 = 128.0;` / `DEFAULT_VOLUME_FAR = DEFAULT_GRID_FAR_METERS * WORLD_UNITS_PER_METER`
  - `crates/renderer/src/vulkan/upscaling.rs:137` — `grid_far_meters: 128`
  - `crates/core/src/lighting.rs:16` — `BETHESDA_UNITS_PER_METER: f32 = 70.0`
  - `crates/renderer/src/vulkan/context/draw.rs:3509` — `DEFAULT_VOLUME_FAR` is the live fallback when `self.volumetrics` is `None` (reachable: `context/init.rs:959` sets it to `None` on a froxel-layout init failure), so a drifted copy is a behavioural divergence from `--fog-grid-far-m`, not only cosmetic
  - The three values currently agree (128 / 128 / 8 960 = 128 × 70) — this is a guard, not a live bug
- **Impact**: This is the #3117 failure shape one axis over: #3117 was filed
  because a stated default (the ledger's froxel cost) silently diverged from
  the live one after a retune. `froxel_xy_divisor` and `froxel_z_slices` are
  now pinned to the config by `froxel_extent_uses_render_resolution_and_configured_divisor`
  and the memory-budget test; `grid_far_meters` is the one member of
  `VolumetricsConfig` with duplicate literals and no pin.
- **Suggested Fix**: Either derive — `DEFAULT_GRID_FAR_METERS` becomes
  `VolumetricsConfig::default().grid_far_meters as f32` and `VOLUME_FAR`
  becomes `DEFAULT_GRID_FAR_METERS * BETHESDA_UNITS_PER_METER` (blocked only
  if `shader_constants_data.rs` must stay dependency-free for `build.rs`, in
  which case) — or add a three-line test in `volumetrics.rs`'s `tests` module
  asserting all three agree, alongside the existing budget test.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D16-05

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
