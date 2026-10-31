# CHAR-2026-10-31-L-02: `send_player_to_jail` falls back to `CrimeModel::SKYRIM` when no model is installed, unlike every sibling in `jail.rs`

Labels: low,character,bug,game:skyrim

Source: `docs/audits/AUDIT_CHARACTER_2026-10-31.md` (CHAR-2026-10-31-L-02). Severity LOW. Dimension: Progression & Pools (Crime). Game: Skyrim (reachable only if a non-Skyrim load order runs the native). Introduced by `36d7bfbd6`.

**Location**: `crates/scripting/src/jail.rs:371`

## Source
`CharacterRulesProfile::crime()` is `None` on FO3/FNV/FO4/FO76/Starfield and `install_crime_model` removes the resource there (pinned by `crime.rs` `install_tests`). `pay_fine`, `pay_fine_thief` and `go_to_jail` (`jail.rs:237, 260, 275`) decline when the resource is absent.

## Description
`world.try_resource::<CrimeModel>().map_or(CrimeModel::SKYRIM, |model| *model)` substitutes a Skyrim ruleset for "no CHARAL crime model for this game", which `character/mod.rs` doctrine forbids ("`None` means no CHARAL for this game, never use the default"). The path is gated by `JailTable` and `CrimeFactionData` matches so it is unlikely to fire outside Skyrim (not verified for FO4's Papyrus surface).

## Impact
Latent: a sentence length computed from Skyrim's divisor on a game that models no crime.

## Suggested Fix
`let Some(model) = world.try_resource::<CrimeModel>().map(|m| *m) else { return false };` after raising `ARRT`, as the siblings do.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
