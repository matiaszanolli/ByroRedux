# CHAR-2026-10-31-L-01: The new armor module duplicates the Light Armor skill-scaling constants CHARAL already owns, and the Skyrim capture still says no such module exists

Labels: low,character,bug,game:skyrim,tech-debt

Source: `docs/audits/AUDIT_CHARACTER_2026-10-31.md` (CHAR-2026-10-31-L-01). Severity LOW. Dimension: Coverage & Doctrine. Game: Skyrim.

**Location**: `byroredux/src/armor.rs:34-37` vs `crates/core/src/character/skyrim.rs` (`LIGHT_ARMOR_RATING_COEFF` 0.004, `skyrim_ruleset` DamageResist row, lines 120-131). Docs: `docs/engine/charal-skyrim-ruleset.md:211-239` and `:241-286`; `docs/engine/charal.md:344-346`.

## Source
UESP *Skyrim:Armor* line 154 ("For NPCs the skill coefficient is 1.5 instead of 0.4"). The capture itself says `0.4/100 = 0.004` and `1.5/100 = 0.015` "are the exact same constants" as `LIGHT_ARMOR_RATING_COEFF`.

## Description
Two independent implementations of the same skill-scaling term exist. CHARAL's is a `player_only` multiplier row on `DamageResist` (Light Armor only; NPCs "not modelled"). `armor.rs` models both skills, player 0.4 and NPC 1.5, and never reads the `DamageResist` row. Nothing writes the displayed rating into `DamageResist` either, so that row feeds only a `GetActorValue DamageResist` read and the two can disagree. The capture's closing paragraph ("belongs in a combat/equipment module ... no such module exists yet") is stale, and `charal.md` still presents the row as the Skyrim armor story.

## Impact
None today (same numbers where they overlap). A coefficient correction in one place will not reach the other, and the capture misleads about where armor lives.

## Suggested Fix
When the armor-model row (CHAR-2026-10-31-M-01) lands, derive both from it, or retire the `DamageResist` multiplier row if nothing reads it. Update the two capture paragraphs and the `charal.md` Skyrim line to name `armor.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
