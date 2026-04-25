# SK-D5-04: bhkRigidBody recovery-path warning spam pollutes every cell load

State: OPEN

## Severity
**MEDIUM** — Logging-quality issue and companion to #546 (SK-D5-01 bhkRigidBody parser misalignment). Single sweetroll demo already logs the warning; a full Meshes0 cell load will burst ~14,408 of these.

## Location
- `crates/nif/src/lib.rs:302-312` (block recovery `log::warn!` path)

## Description
When `bhkRigidBody::parse` under-consumes its block (see #546), the block-level recovery kicks in and logs:

```
Block N 'bhkRigidBody' (size 250, offset X, consumed 215):
    NIF claims 3503082814 elements but only Y bytes remain at position Z in W-byte stream
```

A single sweetroll run logs this warning. An interior cell with hundreds of refs and an exterior cell with thousands will spam tens of thousands of lines — quickly drowning all other logs and making cell-load debugging impossible.

## Impact
- Cell-load logs become unreadable.
- CI job logs (once Skyrim integration tests run) will exceed size limits.
- Real issues at other severity levels get hidden.

## Suggested Fix
Preferred: fix #546 (SK-D5-01). That removes the warning at the source.

Fallback (if #546 is blocked): downgrade the bhkRigidBody-specific recovery path to `log::debug!` with a once-per-archive summary at `log::warn!` level showing `{"bhkRigidBody": 14408}` aggregate counts.

## Completeness Checks
- [ ] **SIBLING**: Audit other block recovery paths in `lib.rs:302-317` — are any other block types similarly spammy? (Aggregation pattern should apply to all.)
- [ ] **TESTS**: After fix, run Skyrim sweetroll demo and assert `log::warn!` line count below a threshold (e.g. 10).
- [ ] Depends on / companion to #546.

## Source
Audit `docs/audits/AUDIT_SKYRIM_2026-04-22.md` finding **SK-D5-04**.
