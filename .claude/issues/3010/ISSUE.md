# SCR-D7-01: populate_quest_fragments runs at one of four runtime-install sites

**Issue**: #3010
**Severity**: HIGH
**Dimension**: 7 — Engine Attach & Trigger Wiring
**Labels**: `high,scripting,bug`
**Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-16.md` (Dimension 7 — cell-loader attach path).

**Location**: `byroredux/src/cell_loader/load.rs`:441 (the only call site) · `byroredux/src/asset_provider/script.rs`:85

## Description

`populate_quest_fragments` — the function that fills `QuestStageFragments` from QUST `VMAD` `.pex` bodies — runs at **exactly one of the four runtime-install sites**. Every exterior launch therefore starts with an empty `QuestStageFragments`, and **no QF_ fragment ever executes**.

## Evidence

```
$ grep -rn "populate_quest_fragments" byroredux/ --include="*.rs" | grep -v _tests
byroredux/src/asset_provider/script.rs:85:pub(crate) fn populate_quest_fragments(
byroredux/src/cell_loader/load.rs:441:    crate::asset_provider::populate_quest_fragments(world, &index);
```

Re-verified 2026-08-17: **one definition, one call site.** The exterior/streaming install paths do not reach it.

## Impact

The M47.2 quest-fragment slice is inert on every exterior launch. `quest_fragment_dispatch_system` runs with nothing to dispatch, so the failure is silent — no error, no warning, just no scripted quest progression outdoors.

Interior loads through `cell_loader/load.rs` work, which is why the slice reads as functional in testing.

## Suggested Fix

Hoist the call to a single install point that every launch path reaches — or call it from each of the four sites. The former is preferable: four call sites is how this diverged.

## Related

- `/audit-scripting` Dim 7; the consumer half is `quest_fragment_dispatch_system`

## Completeness Checks
- [ ] **ALL-PATHS**: Every runtime-install path reaches the population, not just interior cell load
- [ ] **SINGLE-SITE**: Preferably hoisted to one site rather than duplicated four times
- [ ] **NOT-SILENT**: An empty `QuestStageFragments` on a plugin that has QF_ scripts logs a warning
- [ ] **TESTS**: A regression test asserts non-empty fragments after an exterior launch

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3010 --json state` when live state is needed.*
