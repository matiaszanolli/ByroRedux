# CHAR-D3-07: leveling.rs docstring calls SkillXp "a future third variant" and the Oblivion attribute bonus "deferred" — both shipped

- **Issue**: [#2946](https://github.com/matiaszanolli/ByroRedux/issues/2946)
- **Finding ID**: `CHAR-D3-07`
- **Labels**: `low,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2946 --json state`.

---

- **Severity**: LOW
- **Dimension**: Leveling & Progression
- **Game**: Skyrim, Oblivion
- **Location**: `crates/core/src/character/leveling.rs:13-20` (module doc) vs `:59-72` (`SkillXp`) and `crates/core/src/character/tes.rs:190` (`oblivion_attribute_bonus`)
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §5 — "`LevelingModel` is now an **enum** with all three shapes"; §3 row 147 — "shipped: `oblivion_attribute_bonus` (+1…+5), `skyrim_skill_xp_to_next` / `_between`"
- **Description**: The module docstring is the first thing a contributor reads in this file, and two of its statements are stale. Line 20 says "Skyrim's per-skill-XP model (`SkillXp`) is a future third variant" — `SkillXp` is a fully implemented variant 46 lines below it, with a `SKYRIM` const, three accessors and a passing test (`skyrim_skill_xp_matches_uesp`). Lines 16-18 describe the classic-TES level-up attribute bonuses as "the deferred leveling-efficiency mechanic (`docs/engine/charal.md` §5), not modelled here" — the "not modelled *here*" is literally true (it lives in `tes.rs`), but calling it *deferred* contradicts `charal.md` §3, which lists `oblivion_attribute_bonus` as shipped, and it invites exactly the duplicate implementation the global instruction against duplicating logic warns about.
- **Evidence**:
  ```rust
  // crates/core/src/character/leveling.rs:20
  //! Skyrim's per-skill-XP model (`SkillXp`) is a future third variant.
  ```
  ```rust
  // crates/core/src/character/leveling.rs:66 — 46 lines later
  SkillXp { xp_base: f32, xp_mult: f32, xp_per_skill_rank: f32, ... },
  ```
- **Impact**: Documentation only, but on the entry-point docstring of the module this dimension owns — the highest-traffic place for this class of rot.
- **Related**: `charal.md` §8's own struck-through-items note about the rollout list having drifted stale (`feedback_audit_findings`)
- **Suggested Fix**: Rewrite lines 13-20 to describe `SkillXp` as the shipped third variant and cross-link `tes::oblivion_attribute_bonus` as the classic-TES level-up mechanic's actual home.

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
