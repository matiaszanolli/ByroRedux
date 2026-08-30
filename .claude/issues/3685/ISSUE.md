# #3685 — PERF-D5-2026-08-30-05: the volumetrics gate-off arm re-clears the whole integrated froxel volume every frame, with no already-cleared latch

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D5-2026-08-30-05`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,renderer,pipeline,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3685

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:2561-2600`
  (`record_neutral_frame`); call sites
  `crates/renderer/src/vulkan/context/post_passes.rs:515` and `:727`
- **Status**: NEW
- **Description**: When `requires_dispatch` returns false (no global medium, no fog
  volumes, no lingering combustion), `record_volumetrics_pass` calls
  `record_neutral_frame`, which issues two image barriers and a full
  `cmd_clear_color_image` over the integrated froxel volume — **every frame, for as
  long as the gate stays off**. The image is already neutral after the first such
  frame; nothing writes it in between.
- **Evidence**: the gate-off arm is unconditional —
  ```rust
  if !vol.requires_dispatch(volumetric_time_seconds, scatter_coef > 0.0, fog_volumes) {
      vol.record_neutral_frame(&self.device, cmd, frame);
  }
  ```
  Contrast the caustic pass ~200 lines above in the same file, which solves exactly
  this with a per-FIF latch: `caustic_skip_clear_decision(ran, self.caustic_cleared_on_skip[frame])`
  returns `(should_clear, next_latch)` so the clear happens once per skip streak,
  and the predicate is a pure, unit-tested function
  (`post_passes.rs:1145-1190`).
- **Impact**: Derived from the shipped `FROXEL_FORMAT` (`R16G16B16A16_SFLOAT`,
  8 B/froxel) and the default grid: 7.4 MB of clear traffic per frame at a
  1280×720 render extent, 22.1 MB at 1080p, **66.4 MB at native 4K** — repeated at
  frame rate, in fog-free cells, to write zeros over zeros. It is inside the
  `volumetrics_ms` bracket, so it is measurable today. Frequency depends on how
  many cells author no fog medium at all, which I have not sampled — hence LOW
  rather than MEDIUM.
- **Related**: `caustic_skip_clear_decision` / `caustic_cleared_on_skip` (`#2507`)
  is the in-repo precedent and the template for the fix.
- **Suggested Fix**: Add a *volumetrics_cleared_on_skip: [bool; MAX_FRAMES_IN_FLIGHT]*
  latch and reuse the `caustic_skip_clear_decision` shape (or lift it into a shared
  pure helper — it is already generic over "ran / already_cleared"). Reset the latch
  wherever the caustic one resets, and on `recreate_on_resize`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
