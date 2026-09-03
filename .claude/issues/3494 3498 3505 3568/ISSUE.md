# #3494 — PHYS-D6-2026-08-27b-03: duplicated #[test] attribute + misattached rationale doc

**Severity**: LOW · **Location**: `crates/physics/src/water.rs:1889-1912`
**Source**: `docs/audits/AUDIT_PHYSICS_2026-08-27b.md` (PHYS-D6-03)

`bbfd742f` inserted the new `#3268` regression test between the `#3114` test's doc comment and its `#[test]` attribute. Net effect: two `#[test]` attributes near each other, the `#3268` doc is attached to the wrong function, and `current_volume_without_a_water_plane_does_not_wind_up_user_force` (HIGH-severity force wind-up guard) has no rationale doc at all. `cargo check -p byroredux-physics --tests` emits a live `duplicated attribute` warning.

**Suggested Fix**: Delete the stray `#[test]`, move the whole `#3268` test (doc+attribute+body) below the force-wind-up test, restore the `#3114` doc to its own function. Vanished warning is the acceptance criterion.

---

# #3498 — SCR-D5-2026-08-27-04: fragment_coverage doesn't tally decline reasons

**Severity**: LOW · **Location**: `crates/scripting/examples/fragment_coverage.rs:1-22` (claim) vs `:155-165` (tally loop)
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`

Module doc promises a decline-reason tally; the tally loop only counts fragments that DID lower (no `else` arm, no `decline_hist` anywhere). 8,986+18,325 declined fragments report as one opaque number. Directly explains why SCR-D5-2026-08-27-01 survived four prior audits.

**Suggested Fix**: Record per-declined-fragment the first unclassified statement shape (method name + arity), print top ~30. Also fix stale docstring at `byroredux/src/asset_provider/script.rs:74-85` ("Runs once per cell load" — actually once per session since #3161).

---

# #3505 — REG-2026-08-27-03: SCAN_ROOTS still misses crates/renderer and crates/save

**Severity**: LOW · **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:362-369` (`SCAN_ROOTS`)
**Source**: `docs/audits/AUDIT_REGRESSION_2026-08-27.md` (REG-2026-08-27-03)

Regression/partial-fix of #3166. Two crates with production `impl Resource for` sites are outside the scan: `crates/renderer/src/vulkan/allocator.rs` (`AllocatorResource`, `GpuMemoryBudget`), `crates/save/src/registry.rs` (`SaveRegistry`). No live bug (all 3 are engine machinery, correctly never-saved) but the guard is silently incomplete for future additions in those crates.

**Suggested Fix**: Add `../crates/renderer/src` and `../crates/save/src` to `SCAN_ROOTS`, enumerate the three types in the exclusion table with justification.

---

# #3568 — REN-2026-08-30-D7-01: no guard asserts hash_gpu_material_fields covers every GpuMaterial field

**Severity**: MEDIUM · **Location**: `crates/renderer/src/vulkan/material.rs` (`hash_gpu_material_fields`), `crates/renderer/src/vulkan/context/mod.rs` (`DrawCommand::material_hash`)
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` (REN-2026-08-30-D7-01)

`MaterialTable::intern_by_hash` dedups on a u64 hash. A `GpuMaterial` field populated by `to_gpu_material` but omitted from the hash walk makes two visually-different materials collapse onto one table slot. Three existing pins are all mutually blind to this (size pin, GLSL-struct-order pin, hash-walks-compared-to-each-other pin). Coverage is complete TODAY (108/108 fields) but nothing guards against future drift; struct has grown 8 times.

**Suggested Fix**: Source-scanning test next to `gpu_material_size_is_432_bytes` — parse `GpuMaterial`'s field names via the existing `parse_rust_struct_fields` helper, extract `mat.<ident>` identifiers from `hash_gpu_material_fields`'s body via the same `include_str!`, assert set equality both directions.
