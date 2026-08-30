# #3794 — SAVE-D6-2026-08-30-04: save-load-roundtrip.md still calls death "the one case wired today", one cycle after the second reconciler shipped

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, save-load, doc-rot, documentation

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-30.md` (Dimension 6 — M45.1 Live Load-Apply, documentation), HEAD `64f64480`
**Finding ID**: `SAVE-D6-2026-08-30-04`

- **Severity**: LOW
- **Status**: NEW
- **Data-Loss Class**: none

## Location

- `docs/engine/save-load-roundtrip.md:188-198` — §6 step 7
- `byroredux/src/save_io.rs:1420-1433` — the actual tail

Created by `fa511bbf` (#3488), which shipped the code without updating the companion doc.

## Description

§6 step 7 reads:

> **Reconcile derived removals**: `combat::reconcile_dead_actor_runtime_state` (`byroredux/src/save_io.rs`, called immediately after step 6, both on success and on an apply failure). … **Death is the one case wired today** …

Two things are now wrong:

1. **There are two reconcilers.** `fa511bbf` added `crate::inventory::reconcile_player_equipped_weapon` in the same tail (`save_io.rs:1431-1433`), which is the whole subject of #3488 and the concrete second instance of the marker-plus-reconciler pattern the doc is trying to teach.
2. **The parenthetical "both on success and on an apply failure"** is true of the dead-actor reconciler (it runs in both arms, `:1420` and `:1446`) but **not** of the new one, which sits inside the `Ok` arm only — a deliberate asymmetry, since the `Err` arm aborts on an admittedly partial overlay, but one the doc now describes incorrectly for the step as a whole.

## Evidence

`docs/engine/save-load-roundtrip.md:188-198` and `save_io.rs:1418-1453` read side by side at HEAD. The doc's own §"What's not covered" (`:210-226`) still frames removal support around `Dead` alone.

## Impact

Documentation only. It matters because this doc is the cross-cutting trace an implementer reads before touching the load tail, and it currently **understates the pattern's adoption at exactly the moment the pattern acquired its second instance** — the point at which "this is a pattern, not a one-off" became demonstrable.

## Suggested Fix

Update §6 step 7 to list both reconcilers, note that the equipped-weapon one runs on the success arm only and why, and adjust §"What's not covered" to say the pattern now has two instances.

**While in the file**, correct `:222-224`'s reference-visibility claim per `SAVE-D6-2026-08-30-01` (#3789) — that line asserts a guarantee the load ordering does not deliver.

## Related

- #3488 (the commit that added the second reconciler without the doc)
- #3022 (the `Dead` reconciler the doc describes correctly)
- #3028 (the previous doc-rot finding against the same file, fixed in `5458522d`)
- #3789 (`SAVE-D6-2026-08-30-01` — the other stale claim in the same document, at `:222-224`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the rest of §6 and §"What's not covered", which were written when `Dead` was the only case
- [ ] **TESTS**: N/A (documentation)
