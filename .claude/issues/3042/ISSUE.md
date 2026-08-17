# FNV-2026-08-16-D9-01: the 14 active_package_is_* / active_*_location PACK selectors are dead

**Issue**: #3042
**Severity**: LOW
**Dimension**: 9 — AI Packages
**Labels**: `low,import-pipeline,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_FNV_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FNV_2026-08-16.md` (Dimension 9 — AI Packages & Procedure Runtimes).

**Location**: `crates/plugin/src/esm/records/misc/pack.rs`:396-… (14 `pub fn`s), re-exported at `crates/plugin/src/esm/records/misc.rs`:67-68 and `crates/plugin/src/esm/records/mod.rs`:60-61

## Description

#2031 collapsed the spawn tail into a single `active_package` resolve plus the `is_sandbox()/is_wander()/…` else-if chain in `byroredux/src/npc_spawn/ai_package.rs`:106-146.

The seven `active_package_is_*` predicates and seven `active_*_location`/`active_*_target` accessors they replaced were **left in place**. A workspace-wide search finds **no call expression** for any of the 14.

`pub` visibility suppresses the dead-code lint, so nothing surfaces it.

## Evidence

Re-verified 2026-08-17 — every surviving reference is a `use` statement or a comment, never a call:

```
$ grep -rn "active_package_is_sandbox\|active_sandbox_location" crates/ byroredux/ --include="*.rs" | grep -v pack.rs
crates/plugin/src/esm/records/mod.rs:60:    active_package_is_patrol, active_package_is_sandbox, …   <- use
crates/plugin/src/esm/records/misc.rs:67:   active_package_is_patrol, active_package_is_sandbox, …   <- use
crates/plugin/src/esm/records/actor/tests.rs:696:  /// … `active_package_is_sandbox` always            <- comment
crates/plugin/src/esm/records/actor/tests.rs:730:  /// … `active_package_is_sandbox` looks up           <- comment
```

Same shape for all seven pairs.

## Impact

~150 lines of unreachable public API that still reads as the live selection mechanism. `/audit-fnv`'s own Dimension 9 entry-point list names these selectors, and a future contributor extending package selection will reasonably edit the dead copy.

**Note a doc consequence**: `.claude/commands/_audit-common.md`'s Sandbox AI row states *"the spawn-tail reads all seven `active_package_is_*`/`active_*_location`/`active_*_target` selector pairs"* — that is now stale, and is the kind of claim that would send an auditor to the dead code.

## Suggested Fix

Delete all 14 plus their two re-export lines, keeping `active_package` and `PackRecord::is_*`. Or, if they are wanted as a public plugin-crate API, add a test exercising each so the intent is recorded.

Update `_audit-common.md`'s Sandbox AI row either way.

## Related

- #2031 (the collapse that orphaned them)
- Overlaps `/audit-tech-debt`'s dead-code dimension (#2982 is the same shape in `quest.rs`)

## Completeness Checks
- [ ] **ALL-14**: Every selector removed (or tested), not a subset
- [ ] **RE-EXPORTS**: The two `use` lines in `misc.rs` and `mod.rs` removed with them
- [ ] **SKILL-DOC**: `_audit-common.md`'s Sandbox AI row corrected — it currently points at the dead path
- [ ] **PATH-GATE**: `.claude/commands/_audit-validate.sh` still passes after the skill edit
- [ ] **TESTS**: `cargo test -p byroredux-plugin` green after removal

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3042 --json state` when live state is needed.*
