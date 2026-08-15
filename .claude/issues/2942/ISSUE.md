# CHAR-D3-03: every leveling constant is hardcoded, shadowing ~2039 parsed-but-unreadable GMSTs

- **Issue**: [#2942](https://github.com/matiaszanolli/ByroRedux/issues/2942)
- **Finding ID**: `CHAR-D3-03`
- **Labels**: `medium,legacy-compat,import-pipeline,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2942 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Leveling & Progression
- **Game**: all
- **Location**: `crates/core/src/character/leveling.rs:75-125` (all five model consts); `crates/core/src/character/skyrim.rs:36` (`SKYRIM_SKILL_USE_CURVE`); `crates/plugin/src/esm/records/index.rs:64` (`game_settings`); `crates/plugin/src/esm/records/global.rs:91` (`parse_gmst`)
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §3 table — "| XP / level curve (`iXPBase`, `iXPLevelUpBase`, …) | **AUTHORED** — `GMST` | pending |"
- **Description**: `charal.md` §3 classifies the XP/level curve as **AUTHORED** data that should be read from parsed `GMST` records, and `LevelingModel::SkillXp`'s own docstring names the three GMSTs it encodes (`fXPLevelUpBase` / `fXPLevelUpMult` / `fXPPerSkillRank`). Every one of them is a hardcoded `f32` literal instead. The parse side is not the blocker — `parse_gmst` runs and `EsmIndex::game_settings` holds them (a real FNV load indexes ~2,039 per `crates/plugin/tests/parse_real_esm.rs`) — but the map is keyed by **FormID**, and no editor-ID accessor exists (contrast `EsmIndex::actor_value_form_id`, which CHARAL already uses for every AVIF). So no CHARAL code path *can* resolve a GMST today even if it wanted to. `charal.md` §8 item 6 scopes the open "GMST sourcing" gap to `actor_value_derive.rs`'s `fAVDSkill*` constants and calls them "the last real AUTHORED gap" — contradicted by §3's own separate, still-`pending` XP-curve row.
- **Evidence**:
  ```rust
  // crates/core/src/character/leveling.rs:119-125 — GMST-named, GMST-shaped, hardcoded
  pub const SKYRIM: Self = Self::SkillXp {
      xp_base: 75.0, xp_mult: 25.0, xp_per_skill_rank: 1.0,
      pool_pick_gain: 10.0, level_cap: 0,
  };
  ```
  ```
  crates/plugin/src/esm/records/index.rs:64: pub game_settings: HashMap<u32, GameSetting>,
  ```
  `grep -rn "game_settings"` outside `index.rs` returns only the dispatch insert
  (`dispatch_global.rs:21`) and test count assertions — zero readers.
- **Impact**: Any mod that retunes the XP curve, the Skyrim per-skill-rank award, or `fSkillUseCurve` is silently ignored — the class of breakage the CHARAL AUTHORED/ENGINE-SUPPLIED split exists to prevent. Reachable today only through `GetXPForNextLevel` (which returns vanilla values under a modded load order); the blast radius becomes the whole progression system the moment leveling is implemented.
- **Related**: `charal.md` §8 item 6 (the acknowledged `fAVDSkill*` half); `/audit-esm` owns the `EsmIndex` accessor surface
- **Suggested Fix**: Add an editor-ID→`GameSetting` accessor on `EsmIndex` mirroring `actor_value_form_id`, then make the per-game builders take a `gmst(&str) -> Option<f32>` resolver alongside the existing `resolve`, falling back to today's literals when a setting is absent (the same resolve-or-skip contract the derived tables already use). Correct `charal.md` §8 item 6's "last real AUTHORED gap" claim either way.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
