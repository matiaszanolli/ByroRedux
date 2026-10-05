# #5317: TD3-2026-10-05-03: feature-matrix.md's gameplay table predates the 2026-10-01 slice closure (P1/P2 "not closed", corpse loot ✗, no P3–P5 rows), and the slice doc contradicts itself on P1

Labels: low,tech-debt,documentation,doc-rot,gameplay
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD3-2026-10-05-03) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `docs/feature-matrix.md:223-231`; `docs/engine/playable-vertical-slice.md:3` vs `:98`
- **Status**: NEW
- **Effort**: small
- **Description**:
  - `ROADMAP.md:260-262` says "**Closed 2026-10-01.** All six phases (P0 input → P5 persistence and soak) were
    closed by live gates", and `playable-vertical-slice.md:3` says "complete — P0–P5 closed".
  - `feature-matrix.md`, the status floor, still says:
    - P1 "~ Core traversal gate passes; not closed";
    - P2 "Core checkpoint, not P2 closure";
    - "Corpse interaction / loot transfer | ✗ | P2 remainder".
  - Corpse looting has shipped:
    - `interaction.rs:1904` `lethal_combat_then_physical_activation_loots_corpse_through_its_collider`;
    - `npc_spawn/loot_appearance.rs`;
    - `inventory.rs:770` `is_loot_source`;
    - the 2026-09-17 Skyrim corpse-loot live validation in the slice doc.
  - The table has no P3, P4 or P5 rows at all.
  - Separately, the slice doc's own P1 section still ends "P1 as a whole is not closed yet" (`:98`), against its
    header.
- **Related**: #4747 (OPEN; the combat-sound row at `feature-matrix.md:228`, which AUDIO-2026-10-05 re-confirms);
  #5108 (CLOSED 10-01, synced only the player-body / dialogue / container rows).
- **Suggested Fix**:
  - Add P3–P5 rows with their gate scripts.
  - Mark P1/P2 closed with ROADMAP's named follow-ons (gamepad, #5161).
  - Flip corpse loot to ✓.
  - Re-word `playable-vertical-slice.md:98` to "gamepad sources are a follow-on, not a P1 blocker".

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it
