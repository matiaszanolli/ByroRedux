# CHAR-2026-09-11-D1-02: no `ROSTER_CASES` entry for the FO76 and Starfield profiles, whose `Stored` model resolves two AVIF EditorIDs falsified only against `Fallout4.esm`

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4094
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4094 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Ruleset Seam & CHARAL Doctrine (test coverage)
- **Game**: FO76, Starfield
- **Location**: `crates/plugin/tests/parse_real_esm.rs:296-354` (the table);
  `crates/core/src/character/profile.rs:132-146` (the two uncovered profiles);
  `crates/plugin/src/esm/records/actor_value_derive.rs:320-333` (the two keys)
- **Source**: n/a — coverage finding, no constant involved.

## Description

`CharacterRulesProfile::FALLOUT76` and `::STARFIELD` both carry
  `npc_stats: NpcStatModel::Stored`, so `derive_stored_actor_values` runs for them
  in production and resolves the literal EditorIDs `"Health"` and `"ActionPoints"`
  against that game's own AVIF table. `ROSTER_CASES` has five entries and neither
  of those two is among them, so both EditorIDs — and the header→profile
  classification for both games — are falsified only against `Fallout4.esm`.
  This is the shape #3172 exists to prevent: the #3169 trap (`Illusion` vs the
  authored `AVMysticism`) was exactly "a string that resolves on one master and
  not on another". `test_paths` already ships `FO76_ENV`/`fo76_data_dir` (#3741)
  and `STARFIELD_ENV`/`starfield_data_dir`, so the plumbing to add the two rows
  exists.

## Evidence

`ROSTER_CASES` labels are exactly `"FNV" | "FO3" | "FO4" |
  "Skyrim" | "Oblivion"`. `character_rules_profile` (`records/mod.rs:163-165`)
  routes `GameKind::Fallout76 → FALLOUT76` and `GameKind::Starfield → STARFIELD`,
  both of which reach `derive_stored_actor_values`'s
  `index.actor_value_form_id("Health" | "ActionPoints")` lookups. A miss there is
  a silent skip (`if let Some(fid)`), and `stamp_actor_values` only inserts
  `ActorVitals` when the Health key is present — the same failure mode #3481
  measured at 54 undamageable FO4 actors.

## Impact

If either game spells those AVIFs differently, every FO76 /
  Starfield actor spawns with no Health key and therefore no `ActorVitals` —
  undamageable, with no diagnostic. Low severity only because neither game has a
  playable actor path yet.

## Related

#3172 (the falsification loop), #3169 (the display-name trap),
  #3481 (the undamageable-actor failure mode), #3741 (the FO76 accessor).

## Suggested Fix

Add two `RosterCase` rows (`derived_rows: None`,
  `authors_actor_values: true`, empty attribute set) so the existing loop asserts
  the header→profile classification and — with a two-line extension — that
  `"Health"` / `"ActionPoints"` resolve against `SeventySix.esm` and
  `Starfield.esm`.

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix