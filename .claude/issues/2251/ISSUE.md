# REN-D22-02: canonical_light_animation_flags silently assumes Skyrim's LIGH layout for Fallout 76 and Starfield too

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2251

**Dimension**: 22 (Per-game canonicalization)
**Location**: `byroredux/src/systems/light_anim.rs:47-53` (`canonical_light_animation_flags` — `match game { GameKind::Fallout4 => ..., _ => SHARED_LIGHT_ANIMATION_MASK }`)
**Status**: NEW

**Description**: `canonical_light_animation_flags` only special-cases `GameKind::Fallout4`; every other variant — including `GameKind::Fallout76` and `GameKind::Starfield`, which have never been individually verified against xEdit/F4Edit-equivalent tooling for this flag layout — falls into the catch-all `_` branch and is assumed to share Skyrim's `SHARED_LIGHT_ANIMATION_MASK` layout. Pre-existing (this function predates Session 62); not previously filed as its own issue.

**Evidence**: `light_anim.rs:48-51` — `let source_animation_mask = match game { GameKind::Fallout4 => LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE, _ => SHARED_LIGHT_ANIMATION_MASK, };` — `GameKind` (per `crates/plugin/src/esm/reader.rs:93`) has 6 variants (Oblivion, Fallout3NV, Skyrim, Fallout4, Fallout76, Starfield); only Fallout4 is distinguished from Skyrim's assumed-shared layout.

**Impact**: If Fallout 76 or Starfield's actual LIGH flag layout diverges from Skyrim's at the flicker/pulse bit positions (unverified either way), light-animation behavior (candle flicker, ambient pulse) would be silently wrong on that game's content.

**Related**: REN-D22-01 (the new shadow-policy decode has the analogous but broader problem — no per-game gate at all)

**Suggested Fix**: verify Fallout 76 and Starfield's actual LIGH flag layout against available format documentation/tooling, and add explicit `GameKind::Fallout76`/`GameKind::Starfield` match arms if either diverges from Skyrim's assumed layout.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
