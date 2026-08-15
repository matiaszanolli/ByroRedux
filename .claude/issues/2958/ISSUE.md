# CHAR-D6-01: character/mod.rs docstring omits 5 of 13 sub-modules, including every ruleset builder

- **Issue**: [#2958](https://github.com/matiaszanolli/ByroRedux/issues/2958)
- **Finding ID**: `CHAR-D6-01`
- **Labels**: `medium,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2958 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Coverage & Doctrine
- **Game**: all
- **Location**: `crates/core/src/character/mod.rs` (module docstring, above the `pub mod` block)
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §5 (the six per-game captures the docstring
  should point at); `docs/engine/charal.md` §8 items 3/5/7 (the shipped family impls)
- **Description**: The CHARAL crate-slice docstring is the entry point every future
  contributor reads first, and the skill flags it as load-bearing because it *names
  files*. It enumerates eight sub-modules — `derived`, `leveling`, `ruleset`,
  `reputation`, `resistance`, `affliction`, `regen`, `components` — out of thirteen
  live ones. The five it never mentions are `attribute`, `skill`, `fallout`,
  `skyrim`, and `tes`: that is the entire attribute/skill roster half *and* all three
  per-game family implementations. A reader working only from this docstring would
  conclude the crate has no ruleset builders at all — `fallout4_ruleset`,
  `falloutnv_ruleset`, `fallout3_ruleset`, `oblivion_ruleset`, and `skyrim_ruleset`
  are invisible, despite being re-exported eleven lines below.
  Three further drifts in the same block:
  (a) it cites only `docs/engine/charal-fo4-ruleset.md` and `charal-fnv-fo3-ruleset.md`,
  though `tes.rs` and `skyrim.rs` ship against `charal-oblivion-ruleset.md` and
  `charal-skyrim-ruleset.md`, and two more captures exist;
  (b) the `leveling` bullet describes only the FO XP-curve and "the TES skill-use
  model (Oblivion: 10 major-skill-ups → level)" — `LevelingModel::SkillXp` /
  `LevelingModel::SKYRIM` is absent, the same class of drift `CHAR-D3-07` found one
  file over in `leveling.rs`, and independent of it;
  (c) the closing paragraph locates per-game work at "the parser boundary … in
  `byroredux_plugin`", which is true of *population* but silently omits
  `build_character_ruleset` (`byroredux/src/npc_spawn.rs`) — the documented **single
  construction site** for the resource this whole module exists to produce.
- **Evidence**: `pub mod` list in `crates/core/src/character/mod.rs` names thirteen
  modules: `affliction`, `attribute`, `components`, `derived`, `fallout`, `leveling`,
  `regen`, `reputation`, `resistance`, `ruleset`, `skill`, `skyrim`, `tes`. The
  docstring's `* [`…`]` bullets cover eight. `pub use fallout::{fallout3_ruleset,
  fallout4_ruleset, falloutnv_ruleset};`, `pub use skyrim::{skyrim_ruleset, …}` and
  `pub use tes::{…, oblivion_ruleset}` all appear below an index that never mentions
  their modules.
- **Impact**: Contributor misdirection at the exact place the skill identifies as
  highest-leverage. The omitted modules are where every per-game *number* lives, so a
  reader onboarding to CHARAL is pointed at the mechanism (`derived`, `leveling`) and
  away from the data (`fallout`, `tes`, `skyrim`) — the inverse of what this
  subsystem's risk profile calls for. No runtime effect.
- **Related**: `CHAR-D3-07` (same drift class, `leveling.rs`, distinct file and
  distinct sentence — both should be fixed together); `CHAR-D6-02`.
- **Suggested Fix**: Add bullets for `attribute`, `skill`, and a single "per-game
  family impls" bullet covering `fallout` / `tes` / `skyrim` with their builder names;
  extend the `leveling` bullet to the three-variant enum; list all six capture
  documents; and name `build_character_ruleset` as the single construction site.

---

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
