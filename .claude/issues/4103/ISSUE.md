# CHAR-2026-09-11-D4-01: `#3767`'s fix declared band order insignificant, but `ActiveAffliction` still stores a raw *index* into that `Vec` and `reevaluate_affliction` indexes it unchecked

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4103
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4103 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Pools, Afflictions, Resistances & Reputation
- **Game**: all
- **Location**: `crates/core/src/character/affliction.rs:66-70` (the new docstring), `:93-104` (`band_for`), `:110-119` (`ActiveAffliction`), `:159-179` (`reevaluate_affliction`)
- **Source**: n/a — a structural contract, not a numeric claim. (The band *numbers* remain PENDING for every game per `charal.md` §4.6.)

## Description

#3767 removed `AfflictionTable`'s "`bands` **must** be sorted ascending by `min_pool`" contract and replaced it with "Band order is not significant; `band_for` selects the highest reached `min_pool`, so authored tables remain correct even when their entries are transcribed in a different order." That is true of the **classifier**, but the per-actor *memory* is order-dependent: `ActiveAffliction.band` is a `Option<usize>` index into `AfflictionTable::bands`, and `reevaluate_affliction` reverses the previous band by direct indexing (`&table.bands[i]`). Under the old sorted contract, index ↔ logical band was a canonical function of the data; under the new one, two tables holding the *same* bands in different order produce different indices for the same band. The old docstring made the index stable by construction; the new one asserts an order-independence the stored state does not have.

## Evidence

`affliction.rs:66-69` — "Band order is not significant … authored tables remain correct even when their entries are transcribed in a different order." Against `affliction.rs:167-171`:
  ```rust
  if let Some(i) = old_band {
      for p in &table.bands[i].penalties {
          avs.mod_temporary(p.avif_form_id, -p.delta);
      }
  }
  ```
  Two concrete consequences. (1) *Wrong reversal*: an actor whose `AfflictionStatus` was written against table order `[200, 600]` (index 1 = the 600-rad band) and is re-evaluated against the same bands in order `[600, 200]` reverses the **200**-rad penalties, leaving the 600-rad `temporary_mod` permanently applied — silent, no crash, and `reevaluate_affliction` stays internally consistent while doing it. (2) *Unchecked index*: `table.bands[i]` panics outright if the table is rebuilt with fewer bands while a stale index is held. `AfflictionStatus` is explicitly earmarked for save serialisation (`byroredux/src/save_io/registry_completeness_tests.rs:196` — "classify as gameplay state when activated"), which is exactly the path that carries an index across a table rebuild.

## Impact

Latent today — no `AfflictionTable` is constructed in production, `affliction_tick_system` has no scheduler registration, and no code path reorders a table mid-session. The blast radius is the moment real per-game tables land *and* `AfflictionStatus` is saved: the invariant that used to prevent this (sorted-ascending) was just removed, and its removal was documented as making order *safe*, which is the opposite of the guidance a future author needs. `band_for_ignores_band_order` reverses the table between two **stateless** `band_for` calls, so it cannot detect the stateful case.

## Related

#3767 (CLOSED — the fix this follows from), CHAR-2026-08-30-D4-01 (the original finding), `charal.md` §4.6, `charal.md` §4.5 (`FactionReputation` keys by FormID rather than index — the shape that does not have this problem)

## Suggested Fix

Either key `ActiveAffliction` by something order-stable (the band's `min_pool`, mirroring how `FactionStanding` keys by `repu_form_id` rather than position), or restore an explicit "band order must be stable for the lifetime of any `AfflictionStatus` that references it" clause on the struct and make the reversal `table.bands.get(i)` instead of `table.bands[i]`. Either way, extend `band_for_ignores_band_order` to a *stateful* case: `reevaluate_affliction` into band 1, reverse `table.bands`, re-evaluate, and assert the `temporary_mod` deltas net to the new band's.

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix