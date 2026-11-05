# PRDR2-2026-11-03-03: Starfield RACE subgraph data decoded with FO4's layout: per-entry `SADD` collapses to one value, role 3 read as Idle

Labels: bug,animation,low,game:starfield,esm-plugin

**Severity**: LOW (Starfield is the last compatibility tier; the data is mis-read, not used wrongly yet in a shipped path)
**Dimension**: ESM / Animation
**Source**: found during the ai-packages-completion PRD review round 2 (2026-11-03, rev 3), re-verified at HEAD `eba979dfb`
**Status**: NEW

## Location
- `crates/plugin/src/esm/records/actor/race.rs:820-823`: `SADD` decoded as one record-level `subgraph_additive`
- `crates/plugin/src/esm/records/actor/race.rs:248-256`: `SubgraphRole::from_raw` maps 3 → `Idle`, 5+ → `Other`

## Description
The RACE subgraph decode follows Fallout 4's layout for every game:
- **FO4** (`wbDefinitionsFO4.pas:11305-11319`): `SADD` is one record-level "Subgraph Additive Race"; `SRAF` role enum 0 MT, 1 Weapon, 2 Furniture, **3 Idle**, 4 Pipboy.
- **Starfield** (`wbDefinitionsSF1.pas:16746-16768`): `SADD` is the first field *inside each* "Subgraph Data" entry (one per entry), and the role enum is 0 MT, 1 Weapon, 2 Furniture, **3 Linking Only**, 4 Pipboy, 5 MT Generation, 6 Weapon Generation, 7 Furniture Generation.

So on Starfield every entry's `SADD` overwrites the previous one (only the last survives as the record's "additive race"), and role 3 is read as `Idle`.

## Impact
`anim_subgraph.rs:121` resolves additive races from `subgraph_additive`; on Starfield it resolves from the wrong race, and role-filtered subgraph selection treats linking-only entries as idle sets.

## Suggested Fix
Decode Starfield's per-entry `SADD` into the pending subgraph entry and give Starfield its role enum, chosen by the record's game variant (the same `GameKind` threading the other dialect splits in `race.rs` use). Re-run `crates/plugin/examples/subgraph_census.rs` on `Starfield.esm` before and after.

## Completeness Checks
- [ ] **SADD**: Starfield entries carry their own additive race; FO4 keeps the record-level field
- [ ] **ROLES**: Starfield role 3 = Linking Only, 5-7 decoded; FO4 3 = Idle unchanged
- [ ] **CONSUMER**: `anim_subgraph.rs` additive lookup reads the per-entry value on Starfield
- [ ] **TESTS**: synthetic RACE fixtures for both layouts; census numbers before/after in the PR

