# NIFAL-D4-2026-08-16-01: finish_partial_import hardcodes furniture: None

**Issue**: #3072
**Severity**: MEDIUM
**Labels**: `medium,nif-parser,bug`
**Source report**: `docs/audits/AUDIT_NIFAL_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_NIFAL_2026-08-16.md` (Dimension 4 — canonical completeness).

**Location**: `byroredux/src/cell_loader/partial.rs`:170

## Description

`finish_partial_import` **hardcodes `furniture: None`**, and the process-lifetime NIF cache propagates the loss into interiors.

```rust
// byroredux/src/cell_loader/partial.rs:170 (re-verified 2026-08-17)
furniture: None,
```

The streaming path drops furniture-marker data; because the imported scene is then cached for the process lifetime, an interior load that hits the cache gets the furniture-less version even though the synchronous path would have populated it.

## Impact

Furniture markers are the input to the M42 sandbox seat system (`sandbox_seat_system`, `SeatReservations`). Losing them on the streaming path means actors cannot find seats in any cell whose meshes were first imported by the exterior streamer.

The cache propagation is what makes it more than a streaming-path gap: the loss becomes sticky and order-dependent, so the same cell behaves differently depending on how it was first reached.

## Suggested Fix

Populate `furniture` in `finish_partial_import` from the same source the synchronous path uses. If the data genuinely is not available on the partial path, the cache entry must record that it is incomplete so a later full import can replace it — a silently-partial cached entry is the worse failure.

## Related

- #3074 (NIFAL-D4-02 — the sibling hardcoded `None` in the same function)
- `byroredux/src/systems/sandbox.rs` (the consumer)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Furniture data is populated at import, never re-derived downstream
- [ ] **CACHE-HONESTY**: A partial import cannot masquerade as complete in the process-lifetime cache
- [ ] **SIBLING**: Every other hardcoded `None` in `finish_partial_import` audited (see #3074)
- [ ] **TESTS**: A regression test imports via the streaming path and asserts furniture survives

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3072 --json state` when live state is needed.*
