# Issue #3161: SCR-D7-2026-08-20-02: #3010 was fixed by adding a second populate_quest_fragments call site with no test, no source pin and no smoke coverage — one refactor from silently reverting

- **Finding ID**: `SCR-D7-2026-08-20-02`
- **Severity**: LOW
- **Labels**: `low,scripting,bug`
- **Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3161

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3161 --json state`.

---

- **Severity**: LOW
- **Dimension**: 7 — Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/exterior.rs`:1050-1058 (the new call + guard) · `byroredux/src/cell_loader/load.rs`:441 (the original) · `byroredux/src/asset_provider/script.rs`:85-149 (the populator) · `crates/scripting/src/fragment.rs`:105-107 (`is_empty`), :60-70 (the two independent maps)
- **Status**: NEW — #3010 is CLOSED and its **behavioural** defect *is* fixed. This is about the **shape** of the fix, which is a distinct, unfiled gap.

## Description

`SCR-D7-2026-08-16-01`'s suggested fix was to move `populate_quest_fragments`
*inside* `populate_scene_runtime` **"so it cannot drift from its three siblings
again"**, plus a `SRC.contains(...)` source pin of the kind `exterior.rs`:455
already uses.

**Neither was done.** Instead a second call site was added at the head of
`ExteriorCellApplyJob::begin`.

Functionally that is sufficient — both exterior entries
(`streaming_helpers.rs`:500 and `exterior.rs`:998) funnel through `begin`, so
exterior sessions now populate. But the drift surface is **unchanged in kind and
larger in degree**: there are now **two** populate sites against **four**
`populate_scene_runtime` sites, and **nothing — no unit test, no source pin, no
smoke gate — would notice if the new one were dropped in a future refactor of the
streaming job.** #3010 was a HIGH that survived every prior audit precisely
because nothing pinned the call, and the fix reproduced that condition one site
wider. **The new call site is one refactor from silently reverting.**

### Riding along: the `is_empty()` guard

The new call is guarded on `QuestStageFragments::is_empty()`, which reads **only**
the `map` field. `populate_quest_fragments` writes **two independent maps** —
`insert_vmad` populates `vmad` for every scripted quest *before* any `.pex` is
resolved, and `insert` populates `map` only on a successful lowering.

So a session where the VMAD side populates but no `QF_` `.pex` resolves (wrong or
missing `--scripts-bsa` — the exact case the smoke harness's own WARN text
anticipates) leaves `map` empty forever, and the full **845-quest walk**, with a
per-quest `HashMap` build and an archive `extract_pex` per script name, re-runs on
**every** exterior cell `begin` for the rest of the session.

## Evidence

```rust
// byroredux/src/cell_loader/exterior.rs:1052 — the guard
if world
    .resource::<byroredux_scripting::QuestStageFragments>()
    .is_empty()
{
    crate::asset_provider::populate_quest_fragments(world, &wctx.record_index);
}
```

```rust
// crates/scripting/src/fragment.rs:105 — what is_empty() actually reads
pub fn is_empty(&self) -> bool { self.map.is_empty() }
// :63 and :69 — the two independent Arc<HashMap>s
map:  Arc<HashMap<(QuestFormId, u16), Vec<Effect>>>,
vmad: Arc<HashMap<QuestFormId, ScriptInstanceData>>,
```

```
$ grep -rn "populate_quest_fragments" byroredux/src
byroredux/src/asset_provider/script.rs:85       (definition)
byroredux/src/cell_loader/load.rs:441           (interior, unconditional)
byroredux/src/cell_loader/exterior.rs:1057      (exterior, is_empty-guarded)
```

— and **no test file references either call site.**

## Impact

**Bounded.** The re-walk is not catastrophic (a `u16`-bounded BSA hash lookup per
quest, a few dozen cells per streaming session), and the behaviour is correct in
every case — only wasteful.

**The durable part is the unguarded second call site.** This is the actionable
half: a fix that re-created the exact structural condition that let the original
HIGH hide, and there is no gate anywhere that would catch a silent revert —
`m47-triggers.sh` is `--cell`-only (#3160), so it does not reach this path even
in principle.

## Related

- **#3010 (CLOSED)** — the behavioural fix; this is its shape
- **#3160** (`SCR-D7-2026-08-20-01`) — the smoke harness that cannot cover this
- #2541 — the same missing-source-pin class

## Suggested Fix

Either consolidate as originally suggested, **or** add the
`SRC.contains("populate_quest_fragments(")` source pin to `exterior.rs`'s existing
pin test module — **one line, and it is the exact mechanism that would have caught
#3010.**

Separately, change the guard from `is_empty()` to a "have we already attempted
this index" latch (a `populated_from: Option<*const EsmIndex>` or a plain `bool`
resource), so *"tried and found nothing"* is distinguishable from *"not yet
tried"*.

---
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-20.md` (finding `SCR-D7-2026-08-20-02`)

## Completeness Checks
- [ ] **SIBLING**: The other three `populate_scene_runtime` sites re-checked for the same one-of-N-sites divergence
- [ ] **TESTS**: A regression test pins this specific fix — the source pin is the minimum; a unit test that the exterior job populates is better
