# #3455 — TD8-2026-08-27-01: PersistentRefIndex is fully dead — inserted at boot, never built, read, or invalidated — and the milestone its allow(dead_code) names as the pending consumer closed on 2026-08-26

Labels: `low,terrain-exterior,tech-debt,bug`
Filed: 2026-08-28 · Source report: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md`

---

**Severity**: LOW · **Dimension**: 8 — Dead Code & Backwards-Compat Cruft · **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md` (TD8-2026-08-27-01)

**Location**: `byroredux/src/components.rs:1335-1358` (struct doc + two field-level allows), `byroredux/src/cell_loader/persistent_ref_index.rs:45,66` (two function-level allows), `byroredux/src/boot.rs:496` (the live insertion)

## Description
The SKILL correctly excludes "landed ahead of its consumer" code from Dim 8 — *note it, do not delete it*. This one has passed the point where that exclusion applies **on its own stated terms**. Both the struct doc and all four `#[allow(dead_code)]` comments name the same two gating milestones:

```rust
/// Landed ahead of its consumer, same posture as `groundcover_translate`'s
/// Phase 0 constants: fully exercised by `cell_loader::persistent_ref_index`'s
/// test suite, a *pending* production consumer (EX-14/15, EX-16) rather than
/// unused code — hence the field-level `#[allow(dead_code)]` below.
pub(crate) struct PersistentRefIndex {
    #[allow(dead_code)] // see the struct doc — EX-14/15/EX-16 is the pending consumer
```

**EX-14/15 is #2369, CLOSED 2026-08-26** — it shipped without wiring the index. Only EX-16 (#2372) remains open, so the justification is now half false at four sites. Meanwhile the resource is inserted into the live `World` at `boot.rs:496` and nothing in production ever calls `resolve_persistent_ref` or `invalidate`; the only callers are its own tests.

## Evidence
Verified at publish time (2026-08-28):

```
$ grep -rn "PersistentRefIndex" byroredux/src --include='*.rs' | grep -v tests | grep -v components.rs
byroredux/src/boot.rs:496:    world.insert_resource(crate::components::PersistentRefIndex::new());
byroredux/src/cell_loader/persistent_ref_index.rs:23:use crate::components::PersistentRefIndex;

$ grep -rn "persistent_ref_index::" byroredux/src --include='*.rs' | grep -v tests
byroredux/src/cell_loader/cell_root_ref_index.rs:40:/// `persistent_ref_index::invalidate`'s own rationale).   # a doc comment, not a call

$ gh issue view 2369 --json state,closedAt -q '.state+" "+.closedAt'
CLOSED 2026-08-26T20:54:20Z
$ gh issue view 2372 --json state -q .state
OPEN

$ grep -n "EX-14/15 (#2369) and EX-16 (#2372)" byroredux/src/cell_loader/persistent_ref_index.rs
45:#[allow(dead_code)] // landed ahead of its consumer — see the module doc; EX-14/15 (#2369) and EX-16 (#2372) are the pending callers
66:#[allow(dead_code)] // landed ahead of its consumer — see the module doc; EX-14/15 (#2369) and EX-16 (#2372) are the pending callers
```

The sibling `CellRootRefIndex` (same file, same pattern, same `boot.rs:497` insertion) is **not** part of this finding — its named consumer is stream-boundary-state-continuity / #3299, which is genuinely still open.

## Impact
Negligible at runtime (one empty `HashMap` resource). The real cost is that a stale "pending" justification is exactly what turns land-ahead-of-consumer code into permanent dead code: the next Dim-8 sweep reads the comment, sees a named milestone, and skips it — as every sweep since the code landed has done.

## Related
#2369 (CLOSED — the milestone that shipped without wiring it), #2372 (OPEN — the remaining gate), #3299 (the sibling's live gate).

## Suggested Fix
Retarget all four `#[allow(dead_code)]` comments and the struct doc to name **only** #2372, so the next reader sees one live gate rather than a closed one; and add a line to #2372 recording that `resolve_persistent_ref` already exists and is waiting on it. If EX-16 is going to reach the index by a different route, delete the resource, the module and the `boot.rs` insertion instead — `form_id_root_index::resolve`, the shared logic underneath, stays live via `CellRootRefIndex`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every other land-ahead-of-consumer `#[allow(dead_code)]` justification naming a milestone — re-check each named issue's state)
- [ ] **TESTS**: A regression test pins this specific fix (if the index is wired, a production-path test; if deleted, the suite still builds without the module)
