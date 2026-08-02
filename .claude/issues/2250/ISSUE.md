# REN-D22-01: Session 62's shadow-policy flag decode bypasses the per-game canonicalization boundary and reads raw TES5 bit layout unconditionally

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2250

**Dimension**: 22 (Per-game canonicalization)
**Location**: `crates/core/src/ecs/components/light.rs:96-106` (`LIGHT_FLAG_SHADOW_SPOTLIGHT`/`_HEMISPHERE`/`_OMNIDIRECTIONAL`/`_MASK` — documented as "raw LIGH DATA flags" per xEdit's TES5 definitions); `byroredux/src/render/lights.rs` (consumes `light.flags & LIGHT_FLAG_SHADOW_MASK` directly, line 184)
**Status**: NEW

**Description**: Session 62 introduced shared shadow policies (commit `1fb79038`) but the shadow-projection flag bits (`0x400`/`0x800`/`0x1000`) are read directly off `LightSource::flags` as raw TES5 bit positions across all six `GameKind` variants, with no equivalent to the `canonical_light_animation_flags(game, source_flags)` boundary function that already exists for the sibling animation-flag case (which explicitly branches per-game, e.g. Fallout 4's differing reserved bits). Shadow-projection flags get no such per-game gate at all.

**Evidence**: `light.rs:91-95` documents these as "raw LIGH DATA flags, not animation behaviors" straight from TES5's layout; contrast with `canonical_light_animation_flags` in `byroredux/src/systems/light_anim.rs:47`, which explicitly matches on `GameKind` before returning a masked value.

**Impact**: Any game whose LIGH flag layout diverges from Skyrim's TES5 layout at these specific bit positions (unverified for Oblivion/FO3NV/FO76/Starfield) would silently decode the wrong shadow-projection type, or none, from raw bits that mean something else in that game's actual record layout.

**Related**: REN-D22-02 (the sibling animation-flag boundary has the analogous but narrower FO76/Starfield gap)

**Suggested Fix**: add a `canonical_light_shadow_flags(game, source_flags)` boundary function mirroring `canonical_light_animation_flags`, and route `lights.rs`'s `casts_shadows` decode through it instead of reading `LIGHT_FLAG_SHADOW_MASK` raw.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
