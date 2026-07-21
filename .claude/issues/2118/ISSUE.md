# D22-1: LIGHT_FLAG_PULSE_SLOW = 0x400 is mis-assigned to the Shadow-Spotlight bit

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2118
**Labels**: bug, animation, medium

**Severity**: MEDIUM (visual-only)
**Dimension**: Light Animation (audit-renderer Dimension 22)
**Location**: `crates/core/src/ecs/components/light.rs` (`LIGHT_FLAG_PULSE_SLOW`), `byroredux/src/systems/light_anim.rs` (`canonical_light_animation_flags`)
**Status**: NEW, CONFIRMED — verified live during the audit against the `fopdoc/Fallout3/Records/LIGH.md` reference: `Pulse Slow = 0x100`, `Spot Shadow = 0x400`. The codebase's `LIGHT_FLAG_PULSE_SLOW = 0x400` is the wrong bit; this is not a per-game divergence as the in-source comment claims, but a flat mis-assignment.

**Description**: `canonical_light_animation_flags` special-cases Fallout 4 to mask out `0x400` (documented in-code as "FO4's Shadow Spotlight bit"), implying other games genuinely use `0x400` for Pulse Slow. External verification shows `0x400` is Spot Shadow across the FO3-lineage layout the codebase itself cites as authoritative for "the relevant prefix" (see `crates/plugin/src/esm/cell/support.rs`). Consequences:
1. Non-FO4 games (Skyrim/FO3/FNV/Oblivion/FO76/Starfield) still decode any Shadow-Spotlight light as Pulse Slow and slow-pulse the whole-scene light — only FO4 was special-cased.
2. Genuine Pulse-Slow lights (authored with the real `0x100` bit) never animate in any game, since no constant matches `0x100`.
3. The FO4 mask (`FLICKER | PULSE` only) also strips the legitimate `0x40` Flicker-Slow bit, so FO4 dying-fire/low-oil lights get zero animation.

**Evidence**:
```rust
// crates/core/src/ecs/components/light.rs
pub const LIGHT_FLAG_PULSE_SLOW: u32 = 0x0000_0400;  // should be 0x0000_0100
// byroredux/src/systems/light_anim.rs
GameKind::Fallout4 => LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE,   // also drops 0x40 Flicker Slow
_ => SHARED_LIGHT_ANIMATION_MASK,                              // includes 0x400 → non-FO4 games still slow-pulse shadow-spots
```
External reference: `fopdoc/Fallout3/Records/LIGH.md` DATA flags table — `0x00000100 Pulse Slow`, `0x00000400 Spot Shadow`.

**Impact**: Visual-only, no crash/corruption. Affects the animated-light slice across every supported game except FO4 (which trades this bug for a different gap — see related finding below).

**Related**: The new regression tests `fallout4_shadow_spotlight_is_not_slow_pulse` / `fallout4_real_flicker_and_pulse_map_to_shared_behavior` correctly test the masking *logic* but inherit and lock in the wrong `0x400` premise — they'll need updating alongside the fix. See also the related fx-light completeness gap and spawn.rs duplication filed separately from the same audit dimension.

**Suggested Fix**: Set `LIGHT_FLAG_PULSE_SLOW = 0x0000_0100`. Once corrected, `0x400` no longer collides with any animation bit, so the FO4 special-case can likely be dropped entirely (all games mask to `SHARED_LIGHT_ANIMATION_MASK`), which also restores FO4 Flicker-Slow animation. Update the `light.rs`/`light_anim.rs` docstrings asserting the false Skyrim-vs-FO4 divergence. **Recommend one more cross-check against Skyrim's specific LIGH layout before shipping** — the UESP Skyrim LIGH page returned HTTP 403 during this audit and could not be independently checked, only the FO3-lineage fopdoc reference.

## Completeness Checks
- [ ] **SIBLING**: Re-verify the fix against Skyrim's specific LIGH DATA layout (UESP page was unreachable during the audit — 403)
- [ ] **TESTS**: Update `fallout4_shadow_spotlight_is_not_slow_pulse` / `fallout4_real_flicker_and_pulse_map_to_shared_behavior` to reflect the corrected bit value; add a case for a genuine `0x100` Pulse-Slow light animating correctly on a non-FO4 game

Filed from `docs/audits/AUDIT_RENDERER_2026-07-20.md`.
