# PERF-D1-02: collect_candidates allocates a fresh SipHash HashMap plus a Vec every frame

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md` (Dimension 1 — CPU hot paths).

**Location**: `byroredux/src/interaction.rs`:817-869 (`collect_candidates`)

## Description

`collect_candidates` allocates a **fresh SipHash `HashMap` plus a `Vec` every frame**.

## Evidence

```rust
// byroredux/src/interaction.rs:817-818 (re-verified 2026-08-17)
fn collect_candidates(world: &World) -> Vec<(EntityId, InteractionKind)> {
    let mut candidates = HashMap::<EntityId, InteractionKind>::new();
```

`HashMap::new()` here is `std`'s — SipHash — over an `EntityId` keyspace, rebuilt per frame, and the function returns a freshly allocated `Vec`.

## Impact

Two allocations plus SipHash over a per-frame per-entity keyspace, on the interaction path. This is exactly the shape `_audit-common.md`'s hot-path hashing rule names — *"SipHash on a per-frame per-entity keyspace"* — applied to a collection the rule's four named members do not cover.

Bounded by the candidate count, so smaller than the #2923 cluster, but the same class.

## Suggested Fix

Use `FxHashMap` and reuse both collections from per-frame scratch rather than reallocating. Keep std hashing anywhere the keys are attacker-controlled — `EntityId` is not.

## Related

- #3058 (PERF-D1-01), #3060 (PERF-D1-03) — the other two per-frame allocations in this file
- #2923 / #2985 / #3045 — the hot-path FxHash rule and its two outstanding gaps

## Completeness Checks
- [ ] **HOT-PATH-RULE**: `FxHashMap` per `_audit-common.md`'s rule; std retained for DoS-facing maps
- [ ] **REUSE**: Both the map and the `Vec` come from reused scratch
- [ ] **SIBLING**: Fixed with #3058 and #3060 as one pass over `interaction.rs`
- [ ] **TESTS**: A bench pins the per-frame allocation count

