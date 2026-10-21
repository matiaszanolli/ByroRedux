# #6284: CHAR-2026-10-21-L-02: The #5921 profile rows have no capture line for Skyrim/FO4/Oblivion; FO4's row cites a capture note that does not exist and states "awards no kill experience" where the capture says FO4 has kill XP

**Labels**: low,character,documentation,doc-rot,game:skyrim,game:fo4,game:oblivion

**Source**: docs/audits/AUDIT_CHARACTER_2026-10-21.md · **Severity**: LOW · **Dimension**: Ruleset Seam / Coverage & Doctrine · **Location**: `crates/core/src/character/profile.rs` rows: `SKYRIM` (`over_encumbered: NoSprint`, `essential_death_time_secs: Some(30.0)`), `FALLOUT4` (`:397` "#5921 capture note: intended, not an oversight"; `:400` `kill_experience: false`; `essential_death_time_secs: Some(30.0)`), `OBLIVION` (`Some(10.0)`); field doc `:240-244`; `docs/engine/charal-{skyrim,fo4,oblivion}-ruleset.md`

**Audit status**: NEW (#5929 added the FO3/FNV sections only)

- **Game**: skyrim, fo4, oblivion
- **Source**: SKILL: "a coefficient no capture supports is a finding". The values are right: they match the executables' setting tables (Executive Summary), `scripting.md:618-622` and the UESP *Carry Weight* note in `encumbrance.rs`. `charal-fo4-ruleset.md:501-502`: the XP multiplier "scales XP from all sources … Survival mode doubles kill XP".
- **Description**: #5921 made these tables CHARAL profile data, but only the FO3/FNV capture carries them. The Skyrim, FO4 and Oblivion captures lack:
  - the bleedout death-time default;
  - the `NoSprint` rule;
  - the barter executable defaults.

  The FO4 encumbrance comment cites a "capture note" that `charal-fo4-ruleset.md` does not contain (no encumbrance section exists). FO4's `kill_experience: false` follows the field doc "whether killing an actor awards experience". That says FO4 awards none, while the capture says it does. The true state is that FO4's kill-XP formula is uncaptured, so the consumer is off.
- **Impact**: Document-first verification cannot check these rows. The FO4 row encodes "unsourced" as a false game fact, which a later reader may treat as settled.
- **Suggested Fix**: Add a short "Bleedout, encumbrance, barter defaults" block to each of the three captures, citing the exe tables. For FO4, state there that over-encumbrance and kill XP are not wired because the rules are uncaptured. Reword the `kill_experience` doc to "whether kill experience is wired (sourced)".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every `CharacterRulesProfile` row has a capture line in its game's ruleset doc)
