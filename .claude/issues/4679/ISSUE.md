# CHAR-2026-09-21-D1-02: Native HUD's per-game pool roster is a GameKind match in a CHARAL consumer

**Severity**: LOW
**Dimension**: Ruleset Seam
**Game**: all

## Description

Which pools a game's character has is per-game ruleset data: Health/Magicka/Stamina, Health/Magicka/Fatigue, or HP/AP. Here it is expressed as a consumer-side `GameKind` match, because `CharacterRulesProfile`/`CharacterRuleset` expose no pool roster. This is the shape #4447 moved off a consumer for body conditions. No FO3-vs-FNV discrimination is needed, so behaviour is correct.

The Oblivion arm is unreachable in production. `actor_value_form_id` resolves only AVIF records, and `Oblivion.esm` authors none. The test feeds synthetic AVIFs.

## Evidence

Verified at HEAD `ee6d3fb39`, `byroredux/src/inventory.rs`:
```rust
fn vital_bar_candidates(game: GameKind) -> &'static [(&'static str, &'static str)] {
    match game {
        GameKind::Skyrim => &[("Health", "Health"), ("Magicka", "Magicka"), ("Stamina", "Stamina")],
        GameKind::Oblivion => &[("Health", "Health"), ("Magicka", "Magicka"), ("Fatigue", "Fatigue")],
        GameKind::Fallout3NV | GameKind::Fallout4 | GameKind::Fallout76 => &[("HP", "Health"), ("AP", "ActionPoints")],
        GameKind::Starfield => &[("HP", "Health"), ("O2", "O2")],
    }
}
```

## Impact

A future FO3/FNV divergence or new family requires a consumer edit invisible to the profile table; the Oblivion arm gives false coverage confidence.

## Related

#4447 (precedent, moved `body_condition_base` onto the profile the same way), CHAR-2026-09-21-D1-01.

## Suggested Fix

Put the pool roster on `CharacterRulesProfile`, as editor-id/label pairs, and have `build_player_vitals` read it. Drop the Oblivion arm, or mark it blocked on #3768's pre-AVIF resolver.

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D1-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
