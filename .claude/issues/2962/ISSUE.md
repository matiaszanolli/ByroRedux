# CHAR-D6-05: combat.rs and stealth.rs hold capture-sourced character constants outside any audit skill's scope

- **Issue**: [#2962](https://github.com/matiaszanolli/ByroRedux/issues/2962)
- **Finding ID**: `CHAR-D6-05`
- **Labels**: `medium,legacy-compat,tech-debt,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2962 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Coverage & Doctrine
- **Game**: Oblivion (`combat.rs`), FO3/FNV (`stealth.rs`)
- **Location**: `crates/core/src/combat.rs`, `crates/core/src/stealth.rs`;
  scope declarations in `.claude/commands/audit-character/SKILL.md` and the CHARAL row
  of `.claude/commands/_audit-common.md`
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md`, "## The Complete Damage
  Formula — closes Marksman/Hand-to-Hand, adds Luck-chained skill + Armor Rating — all
  now BUILT" (`ModifiedSkill = Skill + 0.4×(Luck−50)`; Hand-to-Hand
  `1 + 10.5 × (Strength/100) × (ModifiedSkill/100)`) and "## Melee weapon damage
  (Blade/Blunt) — BUILT" (`× 0.5 × (0.75 + Strength × 0.005) × (0.2 + WeaponSkill ×
  0.015)`); `docs/engine/charal-fnv-fo3-ruleset.md`, "### Sneak Detection (FNV) —
  LOCKED"
- **Description**: Two top-level modules in `crates/core/src` hold constants sourced
  from CHARAL capture documents and derived from CHARAL actor values, but sit outside
  `crates/core/src/character/`. Both are honest about it — each module docstring
  explains the boundary and cites `charal.md` §7 — so this is not undisclosed code.
  The problem is **ownership**: this skill's Scope block and the CHARAL row of
  `_audit-common.md` both declare the crate slice as `crates/core/src/character/`
  only, so an audit run exactly as specified structurally cannot reach either file.
  Dimension 2 verified 26 constants; none of them are these. The affected numbers are
  precisely the kind this audit exists for — `modified_skill`'s `0.4` Luck
  coefficient, `oblivion_weapon_damage_multiplier`'s four coefficients, and
  `oblivion_hand_to_hand_damage`'s cross-term, all engine-hardcoded from UESP with no
  GMST read (the capture itself lists their GMST names: `fDamageStrengthBase=0.75`,
  `fDamageSkillMult=1.5`, `fHandDamageStrengthMult=0.75`, …). `stealth.rs` (487 lines,
  `detection_score` + `classify` + five input enums) is a full transcription of the
  FO3/FNV sneak-detection algorithm with the same status.
  Neither module is named in `crates/core/src/character/mod.rs`, in `charal.md` §7
  ("What stays out of scope" — which lists combat and dialogue as *concepts* but names
  no files), or in `_audit-common.md`'s layout. The result is a third un-owned
  subsystem of the kind `_audit-common.md` already warns about, but one that is not on
  its list.
- **Evidence**: `crates/core/src/lib.rs` declares `pub mod character;`,
  `pub mod combat;`, `pub mod stealth;` as siblings. `crates/core/src/combat.rs:1` —
  "Classic Oblivion combat-damage math (CHARAL-adjacent, not CHARAL itself)";
  `crates/core/src/stealth.rs` — "## Why this lives outside CHARAL". `modified_skill`,
  `oblivion_weapon_damage_multiplier`, `oblivion_hand_to_hand_damage`,
  `detection_score`, `classify` all exist; `grep` for any of them under
  `crates/core/src/character/` returns nothing.
- **Impact**: ~615 lines of per-game gameplay constants with a real capture-document
  provenance sit in an audit blind spot. A wrong coefficient there has the same
  silent-gameplay-drift profile as one inside `character/` — no crash, no failing test
  unless someone wrote it — with the added hazard that the module docstrings' own
  "CHARAL-adjacent" framing reads as *covered by the CHARAL audit* when it is the
  opposite. Both modules are consumer-less today, so the blast radius is deferred, not
  absent.
- **Related**: `CHAR-D6-01` (the `mod.rs` index that would be the natural pointer);
  `CHAR-D6-02` (§4/§7 of `charal.md`); `CHAR-D3-03` (the same hardcoded-vs-GMST
  problem, inside `character/`).
- **Suggested Fix**: Extend the CHARAL slice in `.claude/commands/audit-character/SKILL.md`
  and `.claude/commands/_audit-common.md` to name `crates/core/src/combat.rs` and
  `crates/core/src/stealth.rs` as in-scope "CHARAL-adjacent" files, add a
  Dimension-2 line item for their constants, and add a "see also" pointer from
  `character/mod.rs` and `charal.md` §7.

---

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
