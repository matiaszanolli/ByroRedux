# DELTA-02: insert_resource doc omits the panic-on-poison contract

**Issue**: #1891 · **Labels**: low, ecs, documentation
**From**: docs/audits/AUDIT_INCREMENTAL_2026-07-05.md (DELTA-02) · **Introduced**: aedcba12 (#1837)

World::insert_resource (crates/core/src/ecs/world.rs) now panics via resource_lock_poisoned on a
poisoned prior-value lock, but the rustdoc still says only "Returns the previous value if one
existed" — no # Panics note. Doc-only hygiene gap; peers (remove_resource) share it. Fix: add a
crate-wide # Panics note on the poison-propagating resource methods. Related: #1837, #466.
