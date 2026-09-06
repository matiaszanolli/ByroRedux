# #3923: SK-2026-09-05-D4-01: the GMST leveling overlay is structurally unreachable, and the real-data test pins that as expected

Filed from `docs/audits/AUDIT_SKYRIM_2026-09-05.md` (SK-2026-09-05-D4-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `medium,game:skyrim,legacy-compat,character,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3923 --json state`.

---

**Source**: `docs/audits/AUDIT_SKYRIM_2026-09-05.md` (SK-2026-09-05-D4-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: 4 (load order / CHARAL boundary)
- **Location**: `crates/core/src/character/leveling.rs`
  (`LevelingModel::with_gmst`), `crates/core/src/character/profile.rs`
  (`build_ruleset`), `crates/plugin/src/esm/records/index.rs`
  (`game_setting_float`), `byroredux/src/npc_spawn.rs`
  (`build_character_ruleset`), `crates/plugin/tests/parse_real_esm.rs`
  (`ROSTER_CASES`)
- **Status**: NEW — an additional consequence of #3848, not a restatement of it.
- **Description**: `with_gmst` has exactly one non-identity arm —
  `Self::SkillXp`, which is **Skyrim's** model (`LevelingModel::SKYRIM`);
  every other variant falls through `other => other`. It is called from one
  place, `build_ruleset`, and only *after* the `RulesetBuilder` match. Because
  Skyrim's arm is `None`, `build_ruleset` returns before `with_gmst` ever
  runs. Consequently:
  * `fXPLevelUpBase` / `fXPLevelUpMult` — the only two authored GMSTs any
    ruleset reads — are never read in production for any game;
  * `EsmIndex::game_setting_float` has **zero reachable production
    consumers**. Its sole caller is the `gmst` closure in
    `build_character_ruleset`, which is threaded all the way from the load
    path and then never invoked;
  * the `game_settings` map itself is populated on every load (2 039 entries
    on `Skyrim.esm` per the parser's own census) and read by nothing.

  The second half is what makes this worth its own issue rather than a note
  on #3848: `crates/plugin/tests/parse_real_esm.rs`'s `RosterCase` for Skyrim
  sets `derived_rows: None`, and the test body's `None` arm asserts
  `ruleset.is_none()`. The one real-data test that exercises this boundary
  **pins the broken state as the expected state**, so fixing #3848 requires
  editing a green test, and nothing today can go red if the GMST decode
  regresses.
- **Evidence**: the same test proves the data is present and resolvable — it
  asserts every entry of `SkillSet::SKYRIM` (18 skills) resolves to an
  authored AVIF in `Skyrim.esm` before it reaches the `derived_rows` check.
  So the resolver works, the rosters are correct, and only the builder arm is
  missing.
- **Impact**: no runtime symptom today beyond #3848's own. The cost is a
  silent test gap: a load-time path (`GMST` float decode → leveling curve
  overlay) that is fully plumbed, fully untested against real data, and will
  execute for the first time on the same commit that fixes #3848.
- **Related**: #3848, #3170 (the fix that never reached `main`), #3221.
- **Suggested Fix**: when #3848 adds the `RulesetBuilder::Skyrim` arm, flip
  this case's `derived_rows` from `None` to the measured count in the same
  commit, and add a direct assertion that `game_setting_float("fXPLevelUpMult")`
  resolves on a real master — otherwise the GMST overlay ships with no
  real-data coverage at all.

---

# Dimension 5 — BSA v105 (LZ4)

## Verified clean

* **Full-corpus extraction**: 32 709 NIF entries extracted from
  `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa` with **0 extraction
  failures** during this audit's own sweeps (both archives are v105 with LZ4
  block compression; every entry was decompressed and parsed).
* Texture extraction exercised across `Skyrim - Textures0/6/7.bsa` for the
  MSN probes, including both `BC3_SRGB_BLOCK` and `R8G8B8A8_SRGB` payloads —
  no failures.
* **Numeric sibling auto-load** (`byroredux_bsa::numeric_sibling_paths`)
  behaves correctly for Skyrim's zero-based series: a trailing `0` with no
  digit before it yields `…1 … …9`, so `Skyrim - Textures0.bsa` drags in
  Textures1-8 and `Skyrim - Meshes0.bsa` drags in Meshes1. The explicit
  re-lists in the `skyrim_se` profile are then skipped by #2584's
  `opened_paths` set instead of being opened twice.
* **Last-wins ordering** (#3637) is right for Skyrim — see dropped
  candidate 4.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
