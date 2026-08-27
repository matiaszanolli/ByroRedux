# FNV-2026-08-26-D9-08

**Issue**: #3354
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 9 — AI Packages & Procedures
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/systems/sandbox.rs:170-196`

**Premise verified**: the early-outs are `SandboxSitClip` missing,
`SandboxBehavior` storage missing, `GlobalTransform`/`Furniture` storage
missing, and `seats.is_empty()`. None of them is "every sandboxing actor is
already `Seated`". The seat build runs unconditionally before the actor
loop:
```rust
scratch.seats.clear();
scratch.seat_meta.clear();
for (furn_e, furn) in furn_q.iter() {
    let Some(furn_g) = gq.get(furn_e) else { continue };
    for (idx, marker) in furn.markers.iter().enumerate() {
        if !is_sit_marker(marker) { continue; }
        scratch.seats.push((seat_id, seat_world_transform(furn_g, marker)));
        scratch.seat_meta.insert(seat_id, (marker.local_offset, furn_g.translation));
    }
}
if scratch.seats.is_empty() { return; }
```
`SandboxBehavior` is never removed on seating (only `Seated` is added, and
the actor loop skips on it), so `sandbox_q` stays non-empty and this runs
every frame for the life of the cell. `seat_meta` exists only to feed the
one-shot `log::info!` diagnostic emitted per assignment, yet it is
re-hashed in full every frame.

**Impact**: opt-in (`BYRO_SANDBOX_SIT`), and per-frame cost is
O(furniture entities × markers) plus one `HashMap` insert per sit marker —
a `GlobalTransform::compose` and a hash per seat per frame in a
furniture-dense FNV interior. Not a default-configuration regression, but
it is unbounded work with a trivially available early-out. Distinct site
from #3269 and from D9-07 above.

**Fix sketch**: skip the whole Pass-1 body when every `SandboxBehavior`
actor already carries `Seated` (a cheap `sandbox_q`/`seated_q` count
compare); and build `seat_meta` lazily inside the assignment branch, since
it is only ever read for the entries that produce a log line.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
