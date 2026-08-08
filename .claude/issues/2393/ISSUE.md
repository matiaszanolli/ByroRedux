# #2393 — ECS-D5B-02: The zero-conflict invariant is near-vacuous — only 9 of the schedule's ~53 systems are ever paired, and the non-vacuity pin doesn't check that

- **Severity**: LOW
- **Domain**: ecs
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2393


- **Severity**: LOW
- **Dimension**: 5b — Scheduler Access Declarations (M27)
- **Location**: `byroredux/src/boot.rs:529-1098` (whole `build_scheduler`); pin at `byroredux/src/scheduler_access_tests.rs:135-141`
- **Status**: NEW

**Description**

`build_scheduler` registers 10 parallel systems (`add_to_with_access`) against 43 exclusive ones (`add_exclusive`). Three of the five stages — `Update`, `PostUpdate`, `Physics` — hold exactly one parallel system each, so `access_report` analyzes zero pairs there. Only `Early` (3 systems → 3 pairs) and `Late` (4 → 6 pairs) produce any pairing at all: 9 pairs total. `known_conflict_count() == 0` and `unknown_pair_count() == 0` are therefore trivially satisfied across 60% of the stage chain. This is a consequence of the M27 conflict-resolution pattern being monotone demotion: every analyzer-visible conflict so far was resolved by moving one side to `add_exclusive`. Because the boot guard fails on a conflict but never on a missing parallel system, demotion is always the cheapest way to make the build green, and the parallel batch drains over time. The `#2138` pin anticipates vacuity but guards the wrong quantity: it asserts `report.system_count() > 20`, which counts exclusives too and is satisfied by 46 systems even if all 10 parallel entries were demoted tomorrow.

**Evidence**:

```
$ grep -c 'scheduler.add_to_with_access(' byroredux/src/boot.rs   → 10
$ grep -c 'scheduler.add_exclusive('       byroredux/src/boot.rs   → 43
per-stage parallel: Early 3, Update 1, PostUpdate 1, Physics 1, Late 4
pairs analyzed = C(3,2) + C(4,2) = 9
```
```rust
// scheduler_access_tests.rs:135 — non-vacuity guard counts exclusives too
assert!(report.system_count() > 20, "…would make the invariants below vacuously true");
```

**Impact**

Diagnostic/architectural, not a correctness bug — `RwLock` still enforces safety and the M27 report is honest about what it saw. But the headline "0 unknown / 0 conflicts" overstates the coverage it represents, and the Update stage (1 parallel + 28 exclusive) means the `parallel-scheduler` feature buys almost nothing on the engine's heaviest stage. The guard as written cannot detect continued erosion.

**Related**: #1601 / #1602 (the demotion that closed the last conflict), #2138 (the pin whose non-vacuity check misses this), #2153 + #2269 (both OPEN — exclusive systems whose lock-ordering safety now rests on that exclusive scheduling).

**Suggested Fix**: Tighten the `#2138` pin to also assert a floor on parallel-system count and on analyzed-pair count (e.g. `>= 9`), so a future demotion has to be a deliberate edit to the pin rather than a silent green build.

## Completeness Checks
- [ ] **TESTS**: Tighten `scheduler_access_tests.rs:135-141` to also floor parallel-system count and analyzed-pair count
- [ ] **SIBLING**: Cross-check ECS-D5B-03 (the `add_exclusive_with_access` dead-API finding) — both point at the same demotion pattern and may share a fix PR

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
