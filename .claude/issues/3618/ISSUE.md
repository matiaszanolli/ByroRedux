# #3618 — REN-2026-08-30-D18-02: the save-registry exclusion justification for `CloudSimState` asserts the exact inverse of `#803`'s code

**Labels**: `low,save-load,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3618 --json state`.

---

- **Severity**: LOW
- **Dimension**: Sky / weather / exterior lighting
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:300`
- **Status**: NEW
- **Description**: The not-persisted allow-list entry reads
  `("CloudSimState", "cloud-scroll accumulator, freshly seeded at [0,0] by every apply_worldspace_weather call (see its own #803 doc)")`.
  Both `apply_worldspace_weather` branches do the opposite: they seed it **only when
  absent**, precisely so the accumulator survives.
- **Evidence**:
  - `world_setup.rs:346-348` (WTHR branch) — *"Insert a default-zero state on first
    exterior load only; subsequent loads reuse the existing accumulator so clouds
    resume drift across interior visits"*, implemented as
    `if world.try_resource::<CloudSimState>().is_none() { world.insert_resource(CloudSimState::default()); }`.
  - `world_setup.rs:718-720` (`insert_procedural_fallback_resources`) — the identical
    `is_none()` guard, commented *"same survives-transitions pattern as the WTHR path"*.
  - `cell_loader/sky_params_cleanup_tests.rs:75-93` pins the survival property directly.
- **Impact**: Documentation only today (the cloud scroll offset is cosmetic and
  self-corrects). But this is the justification a future save-completeness reviewer
  reads to decide the resource needs no snapshot entry, and it rests on a property the
  code deliberately does not have — a save/load round-trip does snap the four cloud
  layers back to `[0,0]`, which the stated reason claims already happens every
  worldspace change.
- **Suggested Fix**: Replace the reason with the true one (cosmetic per-frame scroll
  accumulator, wrapped to `[0,1)` by `rem_euclid`, no gameplay observability), or
  register it if the visible snap on load is judged unacceptable.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D18-02

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
