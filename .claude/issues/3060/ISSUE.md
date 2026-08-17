# PERF-D1-03: refresh_action_state clones two HashSets per frame

**Issue**: #3060
**Severity**: LOW
**Labels**: `low,performance,bug`
**Source report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md` (Dimension 1 — CPU hot paths).

**Location**: `byroredux/src/interaction.rs`:681-709 (`refresh_action_state`)

## Description

`refresh_action_state` **clones two `HashSet`s per frame** purely to work around a resource-guard overlap.

## Evidence

```rust
// byroredux/src/interaction.rs:685-686 (re-verified 2026-08-17)
let keys_held = input.keys_held.clone();
let mouse_buttons_held = input.mouse_buttons_held.clone();
```

The clones exist so the input resource guard can be released before the action state is taken — a borrow-scoping workaround, not a data requirement.

## Impact

Two full `HashSet` clones every frame on the input path. Small in absolute terms (held-key sets are tiny), which is why it is LOW — but it is pure overhead for a lifetime problem that has a zero-copy solution.

## Suggested Fix

Scope the two guards so they do not overlap — read what is needed into locals inside a narrow block, or acquire the resources in one `resource_2_mut`-style call as the ECS's TypeId-sorted API supports. Either removes the clone without changing lock ordering.

## Related

- #3058 (PERF-D1-01), #3059 (PERF-D1-02) — same file, same per-frame-waste class

## Completeness Checks
- [ ] **LOCK_ORDER**: Any change to guard scoping preserves TypeId-sorted acquisition
- [ ] **NO-CLONE**: The workaround is removed rather than made cheaper
- [ ] **SIBLING**: Fixed with #3058/#3059 as one `interaction.rs` pass
- [ ] **TESTS**: Existing interaction tests pass; a bench confirms the clones are gone

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3060 --json state` when live state is needed.*
