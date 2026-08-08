# REN-D22-06: Shadow sibling keeps unnamed bits for Oblivion/FO3NV where the animation sibling drops them -- mirrored-pair policy asymmetry

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2517
**Finding ID**: REN-D22-06 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 22 — Light Animation
**Location**: `byroredux/src/systems/light_anim.rs:99` (`canonical_light_shadow_flags`)
**Status**: NEW

## Description
The two canonicalizers apply opposite policies to *unnamed* bits. `canonical_light_animation_flags` deliberately masks out `0x40`/`0x100` for FO4/FO76 precisely because those positions are unnamed in those games' LIGH definitions (the documented rationale: an unnamed bit must not decode into behavior). `canonical_light_shadow_flags` takes the opposite stance for Oblivion/FO3/FNV: it applies the full TES5 `LIGHT_FLAG_SHADOW_MASK` (`0x400|0x800|0x1000`) to them, and its docstring justifies this with an absence-of-evidence argument ("No divergence has been identified"). Of those three bits only `0x400` (Spot Shadow) is a named flag in the Oblivion/FO3/FNV LIGH layouts; `0x800`/`0x1000` have no named meaning there. This is exactly the "per-game divergence added to one and not the other" shape the pair is supposed to be audited for, expressed as a policy split rather than a missing arm.

## Evidence
```rust
// unnamed bits are DROPPED here for FO4/FO76 …
GameKind::Fallout4 | GameKind::Fallout76 => LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE,
// … but KEPT here for Oblivion/Fallout3NV (0x800/0x1000 unnamed in their layouts)
GameKind::Starfield => 0,
_ => LIGHT_FLAG_SHADOW_MASK,
```
`every_game_shares_the_same_shadow_mask_today` pins the permissive behaviour for `Oblivion` and `Fallout3NV` explicitly.

## Impact
An Oblivion/FO3/FNV LIGH that happens to carry `0x800` or `0x1000` (reserved/junk there) is silently promoted to "casts shadows" and gets RT shadow rays. Unconfirmed against real record data — reported as a consistency/policy gap, not a verified data corruption. Blast radius is bounded by however many such records exist (possibly zero).

## Related
#2250 (REN-D22-01), #2251 (REN-D22-02).

## Suggested Fix
Either give Oblivion/Fallout3NV an explicit `=> LIGHT_FLAG_SHADOW_SPOTLIGHT` arm (only the bit their layouts name), or add a sentence to the docstring stating the deliberate asymmetry — that shadow decode is permissive-by-default while animation decode is strict-by-default — so the next auditor doesn't read it as drift.

## Completeness Checks
- [ ] **TESTS**: If the arm is narrowed, `every_game_shares_the_same_shadow_mask_today` is updated to assert the new per-game behavior
