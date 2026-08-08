# REN-D14-2026-08-07-01: Parked-camera caustic EMA has no dynamic-scene invalidation

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2468
**Finding ID**: REN-D14-2026-08-07-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 14 — Caustics
**Location**: `crates/renderer/src/vulkan/caustic.rs::CausticPipeline::dispatch` (the `camera_static` branch) / `crates/renderer/src/vulkan/context/draw.rs:1740` (`camera_static` derivation)
**Status**: NEW

## Description
The temporal-EMA path that replaced the per-frame clear is gated on a single global boolean derived **only** from the jitter-free view-projection matrix. When that matrix is unchanged, the accumulator is not cleared; instead it is scaled by `decay = parked_frames/(parked_frames+1)` (capped at `CAUSTIC_DECAY_MAX = 0.995`) and this frame's splat contributes only `emaWeight = 1 - decay`. There is no per-pixel motion-vector, mesh-ID, normal-consistency or light-change invalidation anywhere in the path — unlike `svgf_temporal.comp` and `taa.comp`, both of which reject history per-pixel. A parked camera with a *moving scene* therefore keeps up to `1/(1-0.995) = 200` frames of stale pool: a swinging/carried lantern, a walking NPC with a torch, an occluder crossing between the light and the glass, physics clutter settling, or a glass door opening all change every landing point while the accumulator still holds the old pool at up to 99.5% weight.

## Evidence
`draw.rs:1740`:
```rust
let camera_static = vp.iter().zip(self.prev_view_proj.iter())
    .all(|(a, b)| (a - b).abs() < 1.0e-6);
```
`caustic.rs::dispatch` — the only consumer:
```rust
if camera_static { self.parked_frames = self.parked_frames.saturating_add(1); }
else { self.parked_frames = 0; }
let decay_factor = if camera_static { (n / (n + 1.0)).min(CAUSTIC_DECAY_MAX) } else { 0.0 };
```
The clear (`cmd_clear_color_image`) is in the `else` (moving-camera) arm only.

## Impact
Visual only, but directly visible: a caustic ghost/trail that persists for ~3s at 60fps whenever the player stands still and something in the scene moves. Worst in exactly the content the feature targets — FNV/Skyrim interiors with chem glass and bottles lit by carried torches and patrolling NPCs. Blast radius is limited to the caustic term (composited additively over `direct`), so no correctness/stability risk.

## Related
#2239 (the other half of the EMA correctness work); the module doc's "On camera motion the host clears... so a stale, mis-registered pool can't smear" comment describes camera motion only.

## Suggested Fix
Either (a) reset `parked_frames` when the scene changes as well as when the camera does — e.g. thread a "scene dirty" signal (moved light / moved caustic-source instance count-or-transform hash) into `dispatch` alongside `camera_static`, or (b) hard-cap `CAUSTIC_DECAY_MAX` far lower (e.g. 0.9, ≈10-frame memory) so a stale pool decays within a few frames while still killing the jitter stipple.

## Completeness Checks
- [ ] **TESTS**: A regression test (or documented manual repro) confirms a scene-dirty signal invalidates the caustic accumulator while the camera stays parked
