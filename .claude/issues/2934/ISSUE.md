# CHAR-D1-03: the FNV/FO3 skill auto-calc rule lives outside CharacterRuleset (spec's skill_calc field never implemented)

- **Issue**: [#2934](https://github.com/matiaszanolli/ByroRedux/issues/2934)
- **Finding ID**: `CHAR-D1-03`
- **Labels**: `low,legacy-compat,tech-debt,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2934 --json state`.

---

- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: FO3 / FNV
- **Location**: `crates/core/src/character/ruleset.rs` (`CharacterRuleset` — 4 fields) vs `crates/plugin/src/esm/records/actor_value_derive.rs` (`SPECIAL`, `special_index`, `SKILL_BASE`, `SKILL_ATTR_MULT`, `SKILL_LUCK_MULT`, `base_skill`, `derive_npc_actor_values`)
- **Status**: NEW
- **Description**: Two related structural gaps, both making a per-game character
  *rule* live somewhere other than the ruleset seam:
  1. `docs/engine/charal.md` §5 declares `CharacterRuleset` with a
     `skill_calc: SkillDerivation` field ("base / attr-mult / luck-mult (from
     GMST)"). The shipped struct has four fields — `attributes`, `skills`,
     `derived`, `leveling` — and **no `SkillDerivation` type exists anywhere in
     the workspace** (`grep -rn "SkillDerivation"` matches only the design doc).
     The three constants it was meant to hold are hardcoded `f32` literals in the
     parser crate, reached through `matches!(game, GameKind::Fallout3NV)`. So the
     FNV/FO3 half of the ruleset is a *code* seam in `crates/plugin`, not a data
     seam in `CharacterRuleset`. (`charal.md` §8 item 6 tracks the *GMST
     sourcing* half of this as open; the missing struct field and the seam's
     location are not covered there, and §2's tier table sanctions the plugin-side
     location, so the spec is internally inconsistent about where this belongs.)
  2. The same file re-declares the Fallout attribute roster as a local
     `const SPECIAL: [&str; 7]`, in the same order and with the same EditorIDs as
     the canonical `AttributeSet::FALLOUT`, plus a hardcoded `special[6]` for
     Luck. `SkillSet::FALLOUT_FO3_FNV`'s governing map was explicitly
     de-duplicated into CHARAL — `fallout.rs`'s module docs call it "the single
     source ... (no local duplicate)" — but the attribute roster beside it was
     not. Two engine-supplied copies of one roster, in two crates, with no test
     tying them together.
- **Evidence**:
  ```rust
  // crates/plugin/src/esm/records/actor_value_derive.rs
  const SPECIAL: [&str; 7] = ["Strength","Perception","Endurance","Charisma",
                              "Intelligence","Agility","Luck"];
  const SKILL_BASE: f32 = 2.0;       // fAVDSkill<name>Base
  const SKILL_ATTR_MULT: f32 = 2.0;  // fAVDSkillPrimaryBonusMult
  const SKILL_LUCK_MULT: f32 = 0.5;  // fAVDSkillLuckBonusMult
  ```
  `AttributeSet::FALLOUT.members()` is that exact list, in that exact order
  ("ordering is the canonical SPECIAL order").
- **Impact**: Maintainability / doctrine drift, not a live miscomputation — the
  two rosters agree today and the three constants match their cited geckwiki
  values. The cost is that the FNV/FO3 skill-derivation rule cannot be varied per
  game, cannot be fed from parsed GMSTs without editing parser code, and a second
  family adopting auto-calc adds another `GameKind` arm rather than another table
  row. A future reorder of `AttributeSet::FALLOUT` would leave the duplicate
  silently stale.
- **Source**: `docs/engine/charal.md:246-254` (the `CharacterRuleset` struct
  listing including `skill_calc: SkillDerivation`); `docs/engine/charal.md:530-535`
  (§8 item 6, the GMST-sourcing half, already known-open).
- **Related**: `charal.md` §8 item 6.
- **Suggested Fix**: Introduce `SkillDerivation { base, attr_mult, luck_mult }` as
  a `CharacterRuleset` field populated by the Fallout builders, have
  `derive_autocalc_actor_values` read it from the ruleset instead of local
  constants, and drop the local `SPECIAL` array in favour of
  `AttributeSet::FALLOUT.members()`. That also puts the GMST sourcing (§8.6) one
  step from done — the values then have one place to be read into.

---

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
