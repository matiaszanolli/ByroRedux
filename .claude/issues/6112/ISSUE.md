# REN-SD-2026-10-19-06: `acquire_matching_geometry` carries a dead guard, `refcount(handle) == Some(0)`

Labels: low,renderer,tech-debt,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6112

**Severity**: LOW (dead code; the intended property holds by another route) · **Dimension**: Memory/Lifecycle

## Location
`crates/renderer/src/mesh/geometry_sharing.rs` (`acquire_matching_geometry`: `if self.refcount(handle) == Some(0) { return false; }`) against `crates/renderer/src/mesh.rs` `refcount`.

## Description
`refcount` returns `None` for a zero count (`.get(handle).copied().filter(|&rc| rc > 0)`), so `== Some(0)` can never be true. The guard was meant to skip a released handle. Today that case is covered because `drop_meshes` prunes `geometry_cache` (`prune_shared_geometry`) and `self.get(handle)` returns `None` for a released slot, so no wrong geometry is shared. A change to either of those would find the guard silently inert.

## Evidence
`refcount` body as above (verified at publish time); introduced in `b9e961eeb`.

## Impact
None today; a misleading safety check on the sharing path that streaming exercises hard.

## Suggested Fix
Test `self.refcount(handle).is_none()` and keep one of the two checks covered by a unit test that releases the last holder without pruning.

Source: docs/audits/AUDIT_RENDERER_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
