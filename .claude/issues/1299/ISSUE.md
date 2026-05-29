# #1299 — DIM12-DOC: skinning/accel doc rot — validate_refit_inputs ref + capacity off-by-one

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_RENDERER_2026-05-28_DIM12). GitHub is authoritative for current state._

**Severity**: LOW (doc rot, no correctness impact) · **Dimension**: GPU Skinning / Acceleration Structures — documentation
**Source**: `docs/audits/AUDIT_RENDERER_2026-05-28_DIM12.md` (findings D12B-2 + DIM12-DOC-1, bundled)

**1. Nonexistent predicate reference (D12B-2)**
`crates/renderer/src/vulkan/acceleration/types.rs:56-59` — `BlasEntry::built_flags` doc cites `validate_refit_inputs`; the real predicate is `validate_refit_flags`. No `validate_refit_inputs` symbol exists. → replace at `types.rs:58`.

**2. Capacity-comment off-by-one + stale 32768-era figures (DIM12-DOC-1)**
`SKIN_MAX_SLOTS = (MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1 = (196608/144) - 1 = **1364**` (slot 0 reserved), descriptor pool `1364 × 2 × 3 = **8184**`. Comments stating `1365` / `8190` omit the `-1`. Sites: `context/mod.rs:41,66,73`; `byroredux/src/main.rs:873-874`; `scene_buffer/constants.rs:15,20-21,40,45`; `skin_compute.rs:319-323`; `bone_palette_overflow_tests.rs:76`. Some sites still carry pre-#1284 `floor(32768/144)=227` / `32688` figures (now 196608 → 196560). `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME = 1366` is a harmless over-provision (≥ 1364, verified safe) but its comment reads as 1365.

**Risk**: None. Code is consistent (every consumer derives from the single `SKIN_MAX_SLOTS` expression — verified). Only human-readable annotations drifted. **Leave `constants.rs:64-86` intact — it is a deliberate cap-evolution HISTORY-LOG, not stale.**

**Suggested fix**: `validate_refit_inputs → validate_refit_flags`; update off-by-one annotations to `1364 / 8184`; refresh 32768-era figures; fold into the next touch of these files.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in the related path (refit vs batched-build; raster vs compute)
- [ ] **DROP**: if Vulkan objects change, Drop impl still correct
- [ ] **CANONICAL-BOUNDARY**: if the fix touches `material_translate.rs::translate_material` / `Material::resolve_pbr` / import-walk emitter params, per-game logic stays at the NIFAL parser→Material boundary (see /audit-nifal). _(N/A for these skinning/accel findings)_
- [ ] **TESTS**: regression test added for this specific fix
