# CHAR-2026-08-27-D5-04: `EsmIndex::merge_from` adopts the last-merged index's `character_rules` unconditionally, including the empty index substituted on parse failure

- **Issue**: [#3384](https://github.com/matiaszanolli/ByroRedux/issues/3384)
- **Finding ID**: `CHAR-2026-08-27-D5-04`
- **Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `medium,esm-plugin,character,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3384 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: all
- **Location**: `crates/plugin/src/esm/records/index.rs:829-830` (`merge_from`), reached
  from `byroredux/src/cell_loader/load_order.rs:285-290`; the value it overwrites is set at
  `crates/plugin/src/esm/records/mod.rs:194` via `character_rules_profile` (`:143-153`)
- **Status**: NEW
- **Source**: the FO3/FNV `HEDR < 1.0` discriminator is `records/mod.rs:146-147`; its
  robustness on shipped data was measured this session (all six FO3 masters `0.94`; all ten
  FNV masters `1.32`–`1.34`), which is why this is a latent seam rather than a live vanilla
  defect.
- **Description**: `character_rules` is the row that decides, for every actor in the load
  order, which skill roster is used, which Health curve seeds auto-calc NPCs, and which
  `CharacterRuleset` builder runs. `merge_from` takes it wholesale from whichever index was
  merged last:

  ```rust
  self.game = other.game;
  self.character_rules = other.character_rules;
  ```

  Two consequences follow, both silent.

  1. **Parse-failure erasure.** The load-order driver swallows a per-plugin parse failure
     and merges a default index instead:

     ```rust
     let plugin_records = esm::records::parse_esm_with_load_order(&bytes, Some(remap))
         .unwrap_or_else(|e| {
             log::warn!("Record parse failed for '{}': {}", path, e);
             esm::records::EsmIndex::default()
         });
     merged.merge_from(plugin_records);
     ```

     `EsmIndex::default()` carries `CharacterRulesProfile::NONE` (and
     `GameKind::default()` == `Fallout3NV`). If the *last* plugin in the order fails to
     parse, the merged index's profile becomes `NONE`, whose `npc_stat_model()` is
     `NpcStatModel::None` → `derive_npc_actor_values` returns `Vec::new()` for **every**
     actor in **every** cell, and `build_ruleset` returns `None` so no `CharacterRuleset`
     resource is ever inserted. The whole character layer switches off behind a single
     `log::warn!`. The `merge_from` docstring's own justification ("last-write-wins …
     multi-plugin loads always share a single game in practice") is about *plugins*, and
     does not contemplate the empty index the caller can hand it.
  2. **Profile flip.** Even on a successful parse, the FO3-vs-FNV split is decided solely
     by the last plugin's own `HEDR` float. Any last-loaded plugin authored with
     `HEDR < 1.0` on an FNV stack switches the entire load order to
     `CharacterRulesProfile::FALLOUT3` — a different 13-skill roster
     (`SkillSet::FALLOUT3` vs `SkillSet::FALLOUT_NV`) and a different Health curve
     (`90 + 20·END + 10·L` vs `95 + 20·END + 5·L`) for every actor.
- **Evidence**: code as quoted. The `HEDR` census that bounds risk (2):

  ```
  FNV masters: FalloutNV 1.34, GunRunnersArsenal 1.34, LonesomeRoad 1.34,
               OldWorldBlues 1.34, HonestHearts 1.33, CaravanPack/ClassicPack/
               DeadMoney/MercenaryPack/TribalPack 1.32
  FO3 masters: Fallout3, Anchorage, BrokenSteel, PointLookout, ThePitt, Zeta — all 0.94
  ```

  So no vanilla load order can trigger (2); it is reachable only through a third-party
  plugin. Trigger (1) needs no unusual data — only a plugin whose record walk errors.
- **Impact**: (1) is a total, silent loss of the character layer for the whole session,
  with the failure indication being a warn line about a *different* subject (the plugin
  parse). (2) mis-states every FO3/FNV actor's skills and Health. Both are the
  silent-wrong-constant class this audit exists for: no crash, no validation error, and no
  test can currently fail, because nothing asserts that the merged profile is a function of
  the *base master* rather than of whatever merged last. The `index.game` half of the same
  two lines has the same shape and a wider blast radius, but belongs to `/audit-esm`.
- **Related**: #2907 (the categories table that made category merging total — this pair of
  scalar fields sits outside it); D5-02 (also depends on `character_rules` selecting the
  right arm).
- **Suggested Fix**: make the overwrite conditional — keep `self.character_rules` when
  `other.character_rules` is `CharacterRulesProfile::NONE` (and `self`'s is not), which
  fixes (1) with one predicate. For (2), select the profile from the **first** plugin that
  yields a non-`NONE` row (the base master, which is what actually determines the game) and
  log at `warn` when a later plugin would have selected a different one, instead of
  adopting it silently. A test that merges a good FNV index followed by
  `EsmIndex::default()` and asserts the profile is still `FALLOUT_NEW_VEGAS` pins the whole
  class.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_CHARACTER_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `CHAR-2026-08-27-D5-04`._
