# #1318 -- TD-D3: Logic duplication -- read_zstring, Z-up coord-flip, water.rs

_Snapshot as filed 2026-05-29 from /audit-publish (AUDIT_TECH_DEBT_2026-05-28)._

**Severity**: LOW | **Dim 3** — Logic Duplication
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-05-28.md` (TD3-NEW-A, TD3-NEW-B, TD3-NEW-C bundled)
**Domain**: nif-parser | **Effort**: small

**TD3-NEW-A** — `crates/plugin/src/esm/cell/helpers.rs::read_zstring` is a verbatim copy of `crates/plugin/src/esm/records/common.rs::read_zstring`. The cell helpers module was carved out post-Session-34 and the function was copied instead of importing. Delete the duplicate; point callers to `records/common.rs`.

**TD3-NEW-B** — The Z-up→Y-up coordinate flip (`[x, z, -y]`) is re-implemented inline at 4 call sites outside the canonical `crates/nif/src/import/coord.rs::zup_point_to_yup` helper. Per CLAUDE.md policy ("always prioritize improving existing code rather than duplicating logic") each site should call the canonical helper. Locations: `byroredux/src/cell_loader/spawn.rs` (2 sites), `byroredux/src/scene/nif_loader.rs`, `crates/nif/src/import/walk/mod.rs`.

**TD3-NEW-C** — `crates/renderer/src/vulkan/water.rs` calls `device.create_descriptor_pool(...)` directly instead of routing through the `DescriptorPoolBuilder` helper used by all other render passes. Inconsistency raises the friction for descriptor-pool-size audits (you have to know to check `water.rs` separately).

**Fix**: A=delete duplicate, import from records/common; B=replace 4 inline flips with `zup_point_to_yup()`; C=migrate water.rs to `DescriptorPoolBuilder`.

## Completeness Checks
- [ ] **SIBLING**: after B, grep for `[x, z, -y]` or `z, -y` patterns to catch any remaining sites
- [ ] **TESTS**: all existing tests must pass; no new tests needed (behavior-preserving)
- [ ] **CANONICAL-BOUNDARY**: B touches import/walk/mod.rs — confirm no material-translate impact (coord-flip is geometry, not material)
- [ ] **UNSAFE**: no unsafe
