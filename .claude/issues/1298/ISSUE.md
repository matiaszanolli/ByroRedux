# #1298 — C-2: no test pins the bone-palette / SkinSlotPool overflow guard firing

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_RENDERER_2026-05-28_DIM12). GitHub is authoritative for current state._

**Severity**: LOW · **Dimension**: GPU Skinning — test coverage
**Source**: `docs/audits/AUDIT_RENDERER_2026-05-28_DIM12.md` (finding C-2)

**Location**: `byroredux/src/render/bone_palette_overflow_tests.rs:85-107`; `crates/core/src/ecs/resources.rs:944-957` (`returns_none_at_max_skinned`).

**Issue**: The overflow guard (one-shot `overflow_warned` latch + `overflow_attempt_count` increment + `allocate()` returning `None` past `max_slot`) is verified to work, but no test asserts the latch fires / the counter increments. The historical M29 regression was silent truncation past the cap; the guard preventing it is unpinned.

**Risk**: Test gap — a future refactor could drop the warn/counter increment without failing CI.

**Suggested fix**: Add a `SkinSlotPool` unit test (in `skin_slot_pool_tests`) that allocates `max_slot + K` entities across `K` over-cap calls and asserts `overflow_attempt_count() == K` and that a subsequent `allocate()` still returns `None` without panic.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in the related path (refit vs batched-build; raster vs compute)
- [ ] **DROP**: if Vulkan objects change, Drop impl still correct
- [ ] **CANONICAL-BOUNDARY**: if the fix touches `material_translate.rs::translate_material` / `Material::resolve_pbr` / import-walk emitter params, per-game logic stays at the NIFAL parser→Material boundary (see /audit-nifal). _(N/A for these skinning/accel findings)_
- [ ] **TESTS**: regression test added for this specific fix
- [x] **TESTS**: this finding *is* a regression-test addition.
