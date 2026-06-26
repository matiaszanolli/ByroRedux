# TD2-003/004: route inline descriptor writes through descriptors.rs helpers (TLAS + water STORAGE_IMAGE)

_Filed 2026-06-26 as #1752 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1752` for live state)._

**Severity**: LOW · **Dimension**: 2 — Logic Duplication
**Location**: `crates/renderer/src/vulkan/caustic.rs:638-643` ↔ `volumetrics.rs:1028-1033` (TLAS write); `crates/renderer/src/vulkan/water.rs:372-376` (STORAGE_IMAGE write)
**Status**: NEW · **Audit**: TD2-003 + TD2-004 (consolidated — both route inline `vk::WriteDescriptorSet` builders through `descriptors.rs`)

## Description
Two inline descriptor-write leaks that bypass the canonical `descriptors.rs` helper module (93 helper call sites already exist):

1. **TD2-003** — the `WriteDescriptorSetAccelerationStructureKHR` + `push_next` TLAS write is byte-equivalent across caustic.rs and volumetrics.rs (binding 6 vs 2 the only diff). volumetrics.rs:1013 carries a "Mirrors caustic.rs:627" comment = hand-kept-in-sync liability. This is the one `WriteDescriptorSet::default()` family the existing `write_*` helpers deliberately don't cover.
2. **TD2-004** — water.rs builds a STORAGE_IMAGE write inline (`descriptors::write_storage_image(set, 0, &img_info)` is exactly the call caustic.rs:611 / volumetrics.rs:434 already make). `write_storage_image` shipped #1046; this inline copy was added 10 days later — a direct policy regression.

## Suggested Fix
- Add `write_acceleration_structure(dst_set, binding, accel_write: &mut WriteDescriptorSetAccelerationStructureKHR) -> WriteDescriptorSet` to `crates/renderer/src/vulkan/descriptors.rs`; route caustic + volumetrics through it.
- water.rs: one-line swap to `descriptors::write_storage_image`; extend its `use super::descriptors::{...}`.

## Completeness Checks
- [ ] **SIBLING**: no other inline `WriteDescriptorSet::default()` AS-write / storage-image sites remain (8 inline `::default()` total; these are the genuine leaks)
- [ ] **TESTS**: caustic / volumetrics / water passes still bind correctly (descriptor reflection test green)
