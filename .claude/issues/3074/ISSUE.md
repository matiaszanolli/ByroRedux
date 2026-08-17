# NIFAL-D4-2026-08-16-02: the stated blocker for dropping flame_attach_offset is false

**Issue**: #3074
**Severity**: LOW
**Labels**: `low,nif-parser,bug`
**Source report**: `docs/audits/AUDIT_NIFAL_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_NIFAL_2026-08-16.md` (Dimension 4 — canonical completeness).

**Location**: `byroredux/src/cell_loader/partial.rs`:131-139 (the comment) and :162 (the drop)

## Description

The stated blocker for dropping `flame_attach_offset` on the streaming path is **false** — the helper takes `&NifScene`, not `&ImportedScene`.

```rust
// byroredux/src/cell_loader/partial.rs:162
flame_attach_offset: None,
// :164 — "mirroring `flame_attach_offset` above; the sync …"
```

The comment at :131-139 justifies the `None` on the grounds that the extraction helper needs a type the partial path does not have. It does not: the helper's parameter is `&NifScene`, which the partial path *does* hold.

## Impact

Flame attach offsets are dropped on the streaming path for no actual reason — the stated obstacle does not exist. Fire lights and flame effects attached to streamed meshes lose their authored offset.

The comment is the more damaging half: it records the gap as blocked, so a reader concludes it cannot be fixed without a refactor.

## Suggested Fix

Call the helper with the `&NifScene` the partial path already has, and delete the comment. Verify the same reasoning was not applied to the sibling `furniture: None` at :170 (#3072) — the comment at :164 explicitly says it mirrors this one.

## Related

- **#3072 (NIFAL-D4-01 — the sibling `None` at :170, whose comment cites this one as precedent)**

## Completeness Checks
- [ ] **PREMISE**: The false blocker comment is deleted, not merely worked around
- [ ] **SIBLING**: #3072's `furniture: None` re-examined — its justification may rest on the same false premise
- [ ] **CANONICAL-BOUNDARY**: The offset is populated at import, not re-derived downstream
- [ ] **TESTS**: A regression test imports via the streaming path and asserts the offset survives

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3074 --json state` when live state is needed.*
