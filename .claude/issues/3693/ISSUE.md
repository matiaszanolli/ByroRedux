# #3693 — PERF-D9-2026-08-30-05: four declared per-frame renderer scratches are absent from `fill_scratch_telemetry`, against that function's own stated maintenance rule

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D9-2026-08-30-05`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,renderer,test-gap,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3693

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:2090-2206` (the producer, 13 rows); omitted fields at `context/mod.rs:1809`, `acceleration/mod.rs:157`, `acceleration/mod.rs:166`, `water.rs:268`
- **Status**: NEW
- **Description**: `fill_scratch_telemetry`'s doc states the rule explicitly:
  *"every persistent `Vec` scratch declared in this crate must show up here.
  Adding a new scratch field on `VulkanContext` (or its sub-managers) without a
  row added below reintroduces the pre-R6 blind spot where scratches grow with
  zero observability."* Four declared scratches are missing.
  `blend_seen_scratch` is the clearest violation — it is `pub`-documented as
  *"Per-frame scratch … Cleared at the top of the walk; capacity persists across
  frames"* and is cleared at `draw.rs:3436` every frame. It is an `FxHashSet`
  rather than a `Vec`, but the function already emits rows for three hash
  containers (`skin_dispatch_seen_scratch`, `previous_rigid_models`,
  `current_rigid_models_scratch`), so the container type is not the reason.
  `tlas_addresses_scratch` and `tlas_missing_samples_scratch` sit on
  `AccelerationManager` next to `tlas_instances_scratch`, which **is** reported
  via `tlas_instances_scratch_telemetry()`. `WaterPipeline::param_scratch` is a
  per-frame packing buffer in a sub-manager the function never reaches.
- **Evidence**: `rows.push(` appears 13 times in `context/mod.rs`; none names
  any of the four. Declarations:
  ```rust
  // context/mod.rs:1809
  blend_seen_scratch: FxHashSet<(u8, u8, bool, bool)>,
  // acceleration/mod.rs:157
  pub(super) tlas_addresses_scratch: Vec<u64>,
  // acceleration/mod.rs:166
  pub(super) tlas_missing_samples_scratch: Vec<String>,
  // water.rs:268
  param_scratch: Vec<GpuWaterParams>,
  ```
- **Impact**: Small in bytes — `blend_seen_scratch`'s key domain is four engine-
  derived material bits; `tlas_addresses_scratch` is documented as ~64 KB at the
  8k-instance ceiling; `tlas_missing_samples_scratch` is capped at
  `MISSING_BLAS_SAMPLE_LIMIT = 5`. The real cost is the rule itself: an
  observability invariant that is 4/17 violated stops being a guard, and the
  next scratch added has no reason to be added here either. LOW.
- **Related**: #2042 (the same producer's row count drifting out of its doc —
  closed by making the doc defer to this function); #2486 (the shrink half of
  the same cluster policy); #3061 / dim_2 (which touch `blend_seen_scratch` for
  its *hasher*, not its telemetry — different defect, not a dup).
- **Suggested Fix**: Add four `rows.push(ScratchRow { … })` entries, routing the
  two `AccelerationManager` fields through an accessor beside the existing
  `tlas_instances_scratch_telemetry()` and adding one on `WaterPipeline`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
