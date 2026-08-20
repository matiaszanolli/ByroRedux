# Issue #3160: SCR-D7-2026-08-20-01: m47-triggers.sh — the domain's only engine-side gate — has no assertion that can fail on a script-attach regression, and reaches only the interior path

- **Finding ID**: `SCR-D7-2026-08-20-01`
- **Severity**: MEDIUM
- **Labels**: `medium,scripting,bug`
- **Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3160

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3160 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: 7 — Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `docs/smoke-tests/m47-triggers.sh`:29-32 (the SOFT declaration), :76 (`--cell`), :137-146 (the only HARD assertion), :148-168 (the SOFT block) · sibling `docs/smoke-tests/m43-quest-runtime.sh`:56 (also `--cell`)
- **Status**: NEW — the `--cell` half was noted inside `SCR-D7-2026-08-16-01`'s *Impact* paragraph; that finding is now closed as #3010 and the observation went with it. **The harness itself has never been filed.**

## Description

`m47-triggers.sh` exists to prove *"the engine decompiles vanilla `.pex` at cell
load and spawns XPRM trigger volumes on real game data"*. It is the domain's only
engine-side gate on real game data.

Its **only exit-code-affecting assertion** is `entities >= ENTITY_FLOOR` with
`ENTITY_FLOOR=300` against an observed ~1900 — i.e. *"a Skyrim interior loaded"*.

Both M47.2 counts are explicitly SOFT: `recognized == 0` and `triggers == 0` each
print a WARN and leave `hard_fail` untouched.

**Deleting `attach_vmad_scripts` entirely, or breaking `pex_archive_path`'s
`scripts\…\.pex` normalisation so every lookup misses, would leave this harness
green.**

The stated justification — *"their values depend on the cell's content and the
mod load order, not on engine correctness"* — does not hold for the default
invocation, because the cell is pinned (`WhiterunBanneredMare`) and the script
header itself asserts that for that cell *"`REFRs recognized` should be > 0"*. A
deterministic signal is being discarded as if it were nondeterministic.

Separately, both this harness and `m43-quest-runtime.sh` launch with `--cell`, so
**neither reaches the exterior REFR-walk / fragment-population path at all**.

## Evidence

```sh
# :137-146 — the whole HARD gate
if (( entities < ENTITY_FLOOR )); then
    echo "smoke[m47-triggers]: HARD FAIL — entities=$entities < floor $ENTITY_FLOOR …"
    hard_fail=1
else
    echo "smoke[m47-triggers]: PASS — entities=$entities >= $ENTITY_FLOOR"
fi

# :159-163 — the recognition "assertion"
if (( recognized == 0 )); then
    echo "smoke[m47-triggers]: WARN — zero REFRs recognized. …"
fi
```

```sh
# :23-25 — the header's own claim about the default cell
# Cell choice: the default (WhiterunBanneredMare) loads reliably and has
# scripted activators, so `REFRs recognized` should be > 0.
```

## Impact

The one instrument that exercises **decompile → recognize → attach** on real game
data cannot report a regression in any of the three. Every *"the attach path is
live"* statement in this thirteen-report series rests on source reading, not on a
gate.

This is one of **sixteen** "green by construction" instances catalogued this
sweep — row **11** in the table in
[`docs/audits/AUDIT_SUITE_SUMMARY_2026-08-20.md`](../../docs/audits/AUDIT_SUITE_SUMMARY_2026-08-20.md#the-verification-layer-is-still-green-by-construction):
*"Scripting | `m47-triggers.sh` | No exit-code assertion — deleting
`attach_vmad_scripts` leaves it green"*. The pattern, not just this instance, is
the finding worth acting on.

Scope note: the exterior REFR walk itself was checked this pass and **does** share
the interior accumulator and summary line (`exterior.rs`:233 / :1275 →
`load_references_budgeted` → `complete_reference_load`), so the `--cell`
limitation costs coverage of the exterior *fragment-population* path (#3161), not
of REFR attach.

## Related

- #3010 (CLOSED) — the finding whose Impact paragraph carried the `--cell` half
- #2541 — no test pins the `is_primary_synth` gate (same missing-pin class)
- #3008, #3003, #3083 — the same green-by-construction shape in the other harnesses
- `docs/audits/AUDIT_SUITE_SUMMARY_2026-08-20.md` — green-by-construction #11

## Suggested Fix

Promote `recognized == 0` to a **HARD fail when the cell is the pinned default
and `--scripts-bsa` resolved** (leave it SOFT under `BYROREDUX_TRIGGER_CELL`
override, where content genuinely varies) — the script already computes both
values and already distinguishes the override case.

Keep the trigger-volume count SOFT; towns really are sparse.

Add a third invocation with `--grid` / `--radius` so the exterior path is covered
at all.

---
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-20.md` (finding `SCR-D7-2026-08-20-01`)

## Completeness Checks
- [ ] **SIBLING**: `m43-quest-runtime.sh` and the other `docs/smoke-tests/` harnesses re-read for the same "every domain-specific count is SOFT" shape
- [ ] **TESTS**: Verify the promoted assertion actually goes RED — delete `attach_vmad_scripts` locally and confirm a non-zero exit before committing
