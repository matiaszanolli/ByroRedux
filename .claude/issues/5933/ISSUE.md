# PHYS-D5-2026-10-16-01: A body that leaves a marker-only current keeps the marker's last force forever

Labels: medium,physics,water,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:1070` (reset gate), `:1290-1337` (marker arm), `:786-795` (no-marker early return)
- **Status**: NEW (latent since #3114's gate; sits in the function #5698 reworked)
- **Description**: rapier user forces persist across steps until `reset_forces`. The #3114 fix scopes the per-frame reset to `current_flow.is_some() || surface.is_some() || t.prior_wet`. A placed `WaterCurrentVolume` that overlaps no water plane never sets `WaterContact`, so `prior_wet` is false for its occupants. The frame a body stops being inside any marker (it moves out of the box, or the marker streams out and the scan takes the `surfaces.is_empty() && current_volumes.is_empty()` early return) none of the three terms holds, nothing resets, and the last `add_force` stays on the body for the rest of its life. `in_current_prev` already records exactly who was in a marker last scan but is only used for the wake latch.
- **Evidence**: probe, zero gravity, 20 kg ball, +X marker of speed 8, no plane, marker despawned after 30 frames:
  ```
  in marker  frame 29: user_force=(80.78,0,0) linvel=(7.06,0,0)
  after exit frame 59 : user_force=(80.78,0,0) linvel=(11.10,0,0)
  after exit frame 299: user_force=(80.78,0,0) linvel=(27.25,0,0)   // past the authored 8, still rising
  ```
  The force is the last proportional term `m*c*(s - v.d)`; with no further evaluation it becomes a constant push, so the body overshoots the authored current speed without bound (air / low friction). Multibody links are immune (the dry-link pass resets them), plain dynamic bodies are not.
- **Impact**: crates / barrels / corpses-before-activation leaving a river marker region that is not covered by a plane band get a permanent phantom thrust (up to `m*c*s`, 640 N for 20 kg at the defaults). It survives sleep and resumes on the next wake. Friction usually pins a resting body, so the visible cases are airborne or sliding bodies.
- **Trigger Conditions**: an FO3/FNV/Skyrim cell with an XWCU/XPRM current marker whose box extends beyond its plane's footprint or above its water band (the #5129 configuration), and a dynamic body that is awake inside it and then leaves it (pushed, carried by the current out of the far end, or the marker's reference unloads while the body persists).
- **Related**: #3114 (introduced the gated reset), #3268 (`in_current_prev` latch), #5698 (same function, link side only).
- **Suggested Fix**: add `|| in_current_prev.binary_search(&t.entity).is_ok()` to the reset gate (the latch is sorted and survives the quiesced path), and in the no-marker early return reset the forces of the bodies still listed in `in_current_prev` before clearing it (needs an entity -> handle lookup through `RapierHandles`). Add a leave-the-marker test next to `current_volume_without_a_water_plane_does_not_wind_up_user_force`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
