**Severity**: LOW · **Domain**: character (Oblivion skill-use leveling) · **Kind**: doc-rot / provenance

## Problem
Under the owner's 2026-11-20 rule (no rule is sourced from a decoded executable), the Oblivion skill-use path still cites `Oblivion.exe` addresses as the source of its rules. #7310 settled two of them (the AdvancePCSkill surplus and the armor-hit roll); the rest remain:

- `crates/core/src/character/skill_use.rs`: the module doc and cost formula (`0x548030`), the cap at 100 (`0x668b30`), "the executable's defaults supply Minor 1.25, Major 0.75 and Spec 0.75", a zero cost stored as 1.0 (`0x663c50`), the rank-up on cost strictly below the experience (`0x668bc8`), the amount (`0x668c30`), and the test note "the executable's formula is the source".
- `byroredux/src/skill_use_emit.rs`: the stance emitter (`0x671620` / `0x671855`: the one-second timers and the sneak > swim > run branch order), and `sMeditate` as "an executable default".
- `docs/engine/skill-use.md` presents §1 (the model), §2 (the action census) and §5 (levels) as "what `Oblivion.exe` does". §2 also says "Where UESP's wording and the executable differ, the executable's census is the source" (Mercantile per barter by value, not per sale).

## Documented sources (local dumps)
- CS *fSkillUseExp* gives the cost formula: `(fSkillUseFactor · L)^fSkillUseExp`, times the Major or Minor mult, times the Spec mult for a specialization skill. CS *fSkillUseFactor* / *Exp* / *MajorMult* / *MinorMult* / *SpecMult* give the defaults 0.35 / 1.5 / 0.75 / 1.25 / 0.75.
- UESP *Oblivion:Increasing Skills* gives the same formula and defaults, the maximum skill value of 100, the per-action table (26 actions) and the time-based skills: "time (secs) spent running / swimming / undetected while moving in sneak mode".
- UESP *Oblivion:Leveling* covers 10 major-skill points per level, the +1..+5 bonus scale, no rollover of unused bonuses, Health +10% of Endurance, the "Perfection cannot be improved upon" message and the level messages. UESP *Oblivion:First Time Players* covers the "rest and meditate" hint.

## Undocumented (to be declared engine choices, as #7310 did)
- The rank-up boundary: the cost strictly below the banked experience.
- The surplus left banked after a use rank-up.
- One rank per banking call.
- A zero cost at skill 0 stored as 1.0.
- The stance timers' accumulate-and-reset, and the sneak > swim > run order with one stance at a time.
- The amount's `mult` rule. Every fed emitter passes 0 today.
- From §5: the tenth increase's own tally being frozen with it (V13), Endurance read after the picks (V8), and the major count's surplus carrying to the next level.

## Decision recorded
UESP's per-action table is the source of the census. Mercantile (code 29) is unfed (`FED_SKILLS`). Per-sale (UESP) versus per-value (the decode) is decided when its emitter is built. The executable's census is not the source.

## Out of scope
- The player-spellcasting numbers (#7077/#7078), already a pending owner question.
- `docs/engine/music.md` (filed separately).
- ObScript opcode identities.

## Fix
Re-cite every rule above to its CS or UESP page. Label the undocumented ones as engine choices in the code and in `skill-use.md`. Keep the Phase 0 addresses only as historical notes, never as sources. No behaviour change.

## Completeness Checks
- [ ] **SIBLING**: the other Oblivion rule docs (`charal-oblivion-ruleset.md` leveling rows) checked for the same citations
- [ ] **TESTS**: no behaviour change; the existing skill-use tests stay green
