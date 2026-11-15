# #6924 — INC-2026-11-14-12: `step_actor_relocations` allocates every frame while idle

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-12). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: MEDIUM
- **Location**: `byroredux/src/app_step.rs` (`step_actor_relocations`: the `cells` / `loaded` build before the `relocation_step_key` early return)
- **Changed in**: `app_step.rs` (commit `4b71155ca`)

## Description
The `cells` and `loaded` vectors — with the worldspace `String` cloned twice per loaded grid cell — are now built before the idle gate, and the old `!ledger.is_empty()` filter is gone. A radius-7 exterior window is about 450 small allocations per frame doing nothing; `ledger.digest()` also hashes every frame.

## Suggested Fix
Compute the key first (`loaded_cells_key` needs only the grids) and build `cells` / `loaded` only when there is a pending request, a due journey, or a changed key.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (or a counting-allocator / bench pin that the idle path allocates nothing)
