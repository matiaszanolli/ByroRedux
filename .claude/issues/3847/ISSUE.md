# #3847: TD4-2026-09-05-01: `_audit-common.md`'s `crates/sdk` layout row understates the crate ~50× — 282 LOC / 2 files against a live 14,050 LOC / 25 files, in an un-owned crate

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-01) via `/audit-publish`, 2026-09-05. Labels: `medium,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3847 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-01), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: MEDIUM
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/_audit-common.md:93` (also `:81`, the `studio_host.rs` sibling row)
- **Status**: NEW (follow-on to #3457, CLOSED — see Related)
- **Effort**: trivial (≤30 min)
- **Age**: row introduced `21a840d5`-era, filed as #3457 and landed 2026-08-27; drift accrued over the 9 days since.

**Description**
The Project Layout row reads:

```
SDK / Studio:    crates/sdk/src/ (lib.rs + studio.rs, 282 LOC, `21a840d5` 2026-08-25) —
                 renderer-independent tooling surface; `StudioSession` is a Resource.
```

The crate is no longer two files. It is **25 files / 14,050 LOC**:

```
$ ls crates/sdk/src/ | wc -l
25
$ find crates/sdk/src -name '*.rs' -exec wc -l {} + | tail -1
 14050 total
```

with `compatibility.rs` alone at ~3,760 production LOC — a file this very skill's
Phase-1 snapshot names as one of the twelve >2000-production-LOC offenders.
`git log --since=2026-08-25 -- crates/sdk/` shows ~20 feature commits (actor values,
equipment, factions, form lookup, UI/menu state, input mappings, StorageUtil
aliasing) landed after the row was written.

The sibling `studio_host.rs` row (`:81`) has the same defect at smaller scale:
252 LOC claimed, **402** live.

**Evidence — this already misled an audit inside the 90-day window.**
`_audit-common.md` is the shared layout map every audit skill is told to read
instead of re-deriving structure, and `crates/sdk` is on that same file's
**un-owned-subsystems** list (no owner audit skill). The combination means the
only structural signal any auditor gets for this crate is the 282-LOC row.
`/audit-scripting` hit exactly that and had to carry an out-of-band correction:

```
.claude/commands/audit-scripting/SKILL.md:44-49
  … plus a new `crates/sdk` crate (~14k LOC) exposing canonical engine state
  (actor values, equipment, plugin/load-order metadata, faction relationships,
  form lookups, UI/menu state, input mappings) to provider calls. None of this
  is in the **Crates** or **Engine-side wiring** lists below, no dimension's
  entry-points/checklist mentions it …
```

That is a downstream skill compensating in prose for a wrong number in the
shared map — which satisfies the severity table's *"stale baseline that misled
an audit in the last 90 days → MEDIUM"* promotion trigger.

**Impact**
Understates required coverage by ~13.8k LOC on a crate with no owner audit,
whose only listed reviewers are "per-domain owner + `/audit-ecs`". An auditor
budgeting effort from this row will treat `crates/sdk` as a rounding error.
Blast radius is every audit that consults the layout map — i.e. all 28.

**Related**
#3457 (CLOSED — *"`_audit-common.md`'s Project Layout … gives `crates/sdk` no
layout row"*; the row it added is now the misleading artifact, so this is a
follow-on, not a re-file). #3497 (CLOSED — `crates/sdk` unscanned by the save
completeness guard, same blind-spot family). #3744 (CLOSED — consolidated skill
drift; explicitly semantic-claims only, no LOC figures, so no overlap).

**Suggested Fix**
Rewrite the row from the live tree: file count, LOC, and the actual module
groups (`actor_values`, `inventory`, `factions`, `perks`, `compatibility`,
`projection`, `storage`, …) rather than `lib.rs + studio.rs`. Then delete
`audit-scripting/SKILL.md:44-49`'s compensating paragraph, since its whole
purpose is to route around this row. Consider dropping the LOC literal
altogether in favour of "re-run `find crates/sdk/src -name '*.rs' | wc -l`" —
the same treatment #2420 applied to the crate count after it went stale twice.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
