# #5240: CHAR-2026-10-03-D2-01: the GECK's *Stats Tab - NPC* page now sources the NPC scope of four #4450 "scope unsourced" rows, and marks Melee Damage "Not used" for NPCs, while `melee_damage_charal_bonus` applies the actor-general `STR×0.5` to NPC aggressors

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,character,bug,gameplay,combat,game:fnv,game:fo3
- **Source report**: docs/audits/AUDIT_CHARACTER_2026-10-03.md

- **Severity**: LOW. Escalate to MEDIUM if "Not used" is confirmed to mean the engine applies no Melee Damage AV to NPC attacks.
- **Dimension**: Derived Formulas (scope contract) / Coverage & Doctrine
- **Game**: fo3, fnv
- **Location**:
  - `crates/core/src/character/fallout.rs`: the shared Crit/Melee/Unarmed rows, pinned by `fo3_fnv_crit_melee_unarmed_scopes_are_actor_general_pending_a_source`.
  - `docs/engine/charal-fnv-fo3-ruleset.md`: the derived table and the #2937 blockquote.
  - `byroredux/src/combat.rs`: `melee_damage_charal_bonus`.
- **Status**: NEW evidence on known-open #4450 / #2937, both closed as documented. The deferral is not re-filed; the citation it was waiting for now exists.
- **Source**: GECK *Stats Tab - NPC*, section "Reported Stats" (NPC-scoped):
  - "Critical Chance: … Generally the same as the NPC's luck"
  - "Unarmed Damage: … Calculated from the Unarmed skill"
  - "Poison Resistance / Radiation Resistance: … Derived from the NPC's Endurance"
  - "**Melee Damage: Not used.**"
  - The list has no Action Points entry.
- **Description**: The page settles four of the capture's "scope unsourced" cells as actor-general:
  - Crit, Unarmed and the two resists match the code's choice.
  - AP's absence is consistent with the conservative `.player_only()`.

  It also says Melee Damage is "Not used" for NPCs. `MeleeDamageConfig` + `melee_damage_charal_bonus` add `0.5·STR` to every aggressor with the row, and `npc_combat_ai_system` makes NPCs aggressors. "Not used" may refer only to the dialog field, so the line needs adjudication before the ActorGeneral Melee choice is kept or changed.
- **Impact**: Today this is documentation. If "Not used" is engine behaviour, every NPC melee hit is overstated by `0.5·STR` (2.5–5 damage).
- **Suggested Fix**: Cite the page in the capture rows for Crit, Unarmed, Rad and Poison, and in the #2937 note for AP. Then adjudicate the Melee Damage line, with xNVSE `ActorValueOwner` usage or an in-game NPC melee measurement, and flip the row to `.player_only()` if it is confirmed. Update the pin's name and message either way.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all four GECK-sourced rows + the #2937 AP note in the capture)
- [ ] **TESTS**: A regression test pins this specific fix (rename/re-message `fo3_fnv_crit_melee_unarmed_scopes_are_actor_general_pending_a_source`)

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-10-03.md` (finding CHAR-2026-10-03-D2-01, /audit-character 2026-10-03, HEAD `2c36c29d8`).*
