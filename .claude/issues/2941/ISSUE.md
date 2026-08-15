# CHAR-D3-02/D5-03: FO3 silently receives FNV's leveling model; fallout3_ruleset and LevelingModel::FO3 are unreachable

- **Issue**: [#2941](https://github.com/matiaszanolli/ByroRedux/issues/2941)
- **Finding ID**: `CHAR-D3-02`
- **Merged duplicate**: `CHAR-D5-03` (same defect, reached independently by another dimension)
- **Labels**: `medium,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2941 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Leveling & Progression
- **Game**: FO3
- **Location**: `byroredux/src/npc_spawn.rs:157-166` (`build_character_ruleset`); `crates/core/src/character/fallout.rs:110` (`fallout3_ruleset`); `crates/core/src/character/leveling.rs:86-95` (`LevelingModel::FO3`); `crates/plugin/src/esm/reader.rs:93-109` (`GameKind`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md` § *XP / level curve — LOCKED* — "**Level cap:** FO3 **20** … FNV **30**"; "**Perk cadence:** FO3 = 1 perk **every level**; FNV = 1 perk **every other level**"; "`LevelReward` for FO3/FNV = `SkillPoints { base: 10, int_mult: 1.0 (FO3) / 0.5 (FNV), perk_cadence: 1 (FO3) / 2 (FNV) }`"
- **Description**: `build_character_ruleset` maps `GameKind::Fallout3NV` — which covers **both** FO3 and FNV — to `falloutnv_ruleset`, so `fallout3_ruleset` has no call site and `LevelingModel::FO3` is dead outside its own unit tests. The function's docstring justifies the collapse solely on **derived stats**: "the *actor-general* derived stats (Carry Weight / Melee Damage / Crit Chance / Unarmed Damage …) are identical between them". It never mentions that the collapse also substitutes FNV's **leveling model**, and the capture document lists three leveling constants as explicitly divergent: `level_cap` (20 vs 30), `int_mult` (1.0 vs 0.5) and `perk_cadence` (1 vs 2). The XP curve itself is shared (`150·L + 50`), which is why the one live consumer (`GetXPForNextLevel`) does not currently expose the substitution — the divergence hides entirely in methods nothing calls yet.
- **Evidence**:
  ```rust
  // byroredux/src/npc_spawn.rs:161-165
  Some(match game {
      GameKind::Fallout4 => byroredux_core::character::fallout4_ruleset(resolve),
      GameKind::Fallout3NV => byroredux_core::character::falloutnv_ruleset(resolve),
      _ => return None,
  })
  ```
  `grep -rn "fallout3_ruleset"` outside `crates/core/src/character/` returns nothing.
  `GameKind::from_header` *does* receive the discriminating `hedr_version` (FO3 0.85 vs FNV 1.34) before collapsing them, so the information is available at the boundary and is discarded there, not absent.
- **Impact**: Latent today (no leveling runtime consumes `skill_points` / `grants_perk_at` / `level_cap`). Whenever leveling lands, an FO3 load order silently gets half its skill points per level, perks every *other* level instead of every level, and a cap of 30 instead of 20 — with no failing test, because the correct constructor is dead code that nothing exercises against a real game. Distinct from the deferred FO3↔FNV **player Health/AP** item in the known-open register: these are leveling constants, not derived stats.
- **Related**: The known-open FO3↔FNV player Health/AP deferral (`build_character_ruleset` docstring) — adjacent but a different set of values; `/audit-character` Dim 5 owns the derived-stat half of the same collapse.
- **Suggested Fix**: Either thread the already-available HEDR version (or master name) through so `GameKind::Fallout3NV` can select `fallout3_ruleset`, or — if the collapse is intentional for now — extend `build_character_ruleset`'s docstring to state plainly that FO3 inherits FNV's `level_cap` / `int_mult` / `perk_cadence`, and add a test pinning that deliberate divergence so it cannot be mistaken for correctness.

---

### Merged duplicate: CHAR-D5-03

A sibling dimension reached the same defect independently, from the other entry point. Filed once; the second dimension's write-up follows because it adds evidence rather than repeating it.

**The `Fallout3NV` → FNV collapse also swaps the *leveling model*, which the documented justification does not cover — and the correct `fallout3_ruleset` is unreachable**

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fo3
- **Location**: `byroredux/src/npc_spawn.rs` (`build_character_ruleset`) ·
  `crates/core/src/character/fallout.rs` (`fallout3_ruleset`) ·
  `crates/core/src/character/leveling.rs` (`LevelingModel::FO3`, `LevelingModel::FNV`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md`, "XP / level curve — LOCKED":
  *"**Level cap:** FO3 **20** (30 with *Broken Steel*); FNV **30** …"*, *"**Perk
  cadence:** FO3 = 1 perk **every level**; FNV = 1 perk **every other level**"*, and
  *"`LevelReward` for FO3/FNV = `SkillPoints { base: 10, int_mult: 1.0 (FO3) / 0.5
  (FNV), perk_cadence: 1 (FO3) / 2 (FNV) }`"*. The XP curve itself (`150·L + 50`) is
  shared, so only these three constants diverge.
- **Description**: `build_character_ruleset`'s docstring justifies collapsing both
  games onto `falloutnv_ruleset` on the grounds that *"the **actor-general** derived
  stats … are identical between them"*. That claim is **true** — I verified all six
  rows against the capture document (see Scope & Coverage). But the function returns
  the *whole* `CharacterRuleset`, and `CharacterRuleset` also carries
  `leveling: LevelingModel`, which is **not** identical:

  | | `LevelingModel::FO3` | `LevelingModel::FNV` |
  |---|---|---|
  | `level_cap` | 20 | 30 |
  | `int_mult` | 1.0 | 0.5 |
  | `perk_cadence` | 1 | 2 |

  So an FO3 load silently receives FNV's level cap, Skill Rate and perk cadence. The
  divergence is neither mentioned in the docstring nor covered by the known-open
  FO3↔FNV *player Health/AP* deferral, which is scoped to the derived table.
- **Evidence**: `build_character_ruleset` is a two-arm match with no FO3 arm:
  ```rust
  Some(match game {
      GameKind::Fallout4 => byroredux_core::character::fallout4_ruleset(resolve),
      GameKind::Fallout3NV => byroredux_core::character::falloutnv_ruleset(resolve),
      _ => return None,
  })
  ```
  `fallout3_ruleset` exists, is correct, is exercised by
  `fnv_and_fo3_share_skill_stats_but_differ_on_health_ap` and
  `skill_and_attribute_rosters_travel_with_the_ruleset`, and is re-exported from
  `crates/core/src/character/mod.rs` — but a repo-wide grep finds **no production call
  site**. It is dead outside `#[cfg(test)]`. The two green tests give the misleading
  impression the FO3 path is live.
- **Impact**: latent today — the only live `LevelingModel` consumer is
  `GetXPForNextLevel`, which calls `xp_to_next`, and that *is* identical across the two
  games (`150·L + 50`). `level_cap()`, `skill_points()` and `grants_perk_at()` have no
  production caller yet. So nothing is currently wrong on screen; the defect is that
  the first consumer to land will be silently wrong on FO3, and the docstring will
  still read as if the collapse had been justified for it. Blast radius = every FO3
  actor and the FO3 player once chargen/leveling exists.
- **Related**: `CHAR-D1-03` (the ruleset's missing `skill_calc` field — the same
  "ruleset carries more than the derived table" observation from the other side).
- **Suggested Fix**: either (a) narrow the docstring to state plainly that the
  leveling model is knowingly FNV's for both games and name the three divergent
  constants, or (b) do the master-name disambiguation the docstring already
  contemplates for Health/AP — `Fallout3.esm` vs `FalloutNV.esm` is available at load
  order — and route FO3 to `fallout3_ruleset`. (b) also retires the dead-code state.
  Whichever is chosen, add a test asserting which builder `GameKind::Fallout3NV`
  resolves to, so the choice is deliberate rather than incidental.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
