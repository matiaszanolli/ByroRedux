# #2391 — ECS-D5B-03: `add_exclusive_with_access` has zero production call sites — the #1236 declaration channel is unused while 43 exclusives ride on comment-only ordering contracts

- **Severity**: LOW
- **Domain**: ecs, sync
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2391


- **Severity**: LOW
- **Dimension**: 5b — Scheduler Access Declarations (M27)
- **Location**: `crates/core/src/ecs/scheduler.rs:354` + `:382` (the two APIs); `byroredux/src/boot.rs:652-1095` (all 43 exclusive registrations)
- **Status**: NEW

**Description**

`#1236` added `add_exclusive_with_access`/`try_add_exclusive_with_access` specifically so closures and bare `fn` exclusives could declare access. A repo-wide grep finds no caller outside `scheduler.rs`'s own test module — every one of `build_scheduler`'s 43 exclusive registrations uses plain `add_exclusive`, so `undeclared_exclusive_count()` is 43. Per the skill this is by design and NOT a conflict; the follow-on is that the exclusive phase now carries real inter-system ordering contracts that exist solely as prose (several `boot.rs` comments encode "must run before X" dependencies enforced by nothing but registration order). Two OPEN issues — #2153 and #2269 — are the first realised failures of that arrangement.

**Evidence**:

```
$ grep -rn --include='*.rs' 'add_exclusive_with_access' byroredux/src crates tools
crates/core/src/ecs/scheduler.rs:354,382   (definitions)
crates/core/src/ecs/scheduler.rs:1246,1251,1282,1285,1323  (tests only)
crates/core/src/ecs/system.rs:38            (doc link)
→ zero production call sites
```

**Impact**

LOW on its own — exclusives run serially so no race is created by the missing declarations. The cost is that `sys.accesses` shows 43 blank rows for the systems where cross-system ordering/lock disputes are actually happening (#2153, #2269), giving an operator no machine-readable handle on them, and that the API added by #1236 is effectively dead weight.

**Related**: #1236, #1237 (added the API + the per-phase split), #2153 (OPEN), #2269 (OPEN).

**Suggested Fix**: Declare access on the handful of exclusives already known to have cross-system lock/ordering contracts (`pool_regen_tick_system`, the cinematic/quest-stage pair from #2269, the PostUpdate ordering chain) via `add_exclusive_with_access`, so the report can at least name the disputed types; leave the trivial demo dispatchers undeclared.

## Completeness Checks
- [ ] **LOCK_ORDER**: When declaring access on the named exclusives, confirm no newly-surfaced conflict is actually live (they run serially today, but the declaration itself must be accurate)
- [ ] **SIBLING**: Coordinate with #2153 / #2269's own fixes so the declaration lands alongside (not duplicating) their resolutions
- [ ] **TESTS**: A `sys.accesses` snapshot/assertion test confirming the named exclusives now report non-blank access rows

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
