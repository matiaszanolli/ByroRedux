# PHYS-D5-2026-10-16-02: A multibody link inside a marker-only current is "water-owned" but receives no authored damping

Labels: low,physics,water,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:1290-1337` (marker arm), `crates/physics/src/world/mod.rs:1033-1057` (`apply_multibody_link_damping` skip)
- **Status**: NEW (follow-on of the #5698 fix; before the fix the link was damped but the current was wiped)
- **Description**: #5698 registers the body in `water_force_targets` from the current arm so the dry pass stops wiping the marker force. The same registration also makes the dry pass skip the link, and the marker arm adds only `current_force` (a drag toward the flow speed along the flow axis). Unlike the surface arm it folds in no `-c*m*v` / torque for the link's own authored coefficients, which rapier never applies to links (#5540). Net: authored linear and angular damping vanish for a link while it is inside a marker that no plane surface claims.
- **Evidence**: probe, two-body damped ragdoll (linear damping 1.0, mass 10) free-falling for 1 s under default gravity:
  ```
  no marker (dry damping owns)       vy = -436.2
  marker-only current, speed 0       vy = -683.8   // ~ undamped free fall (-686.7)
  ```
  A zero-speed marker isolates the effect: it contributes no force off the flow axis.
- **Impact**: a ragdoll (corpse) falling or tumbling through a placed current volume that overlaps no plane is un-damped for as long as it is in the box (about 57% faster after one second at damping 1.0). Cosmetic-to-moderate fidelity loss; no instability seen. Narrow trigger (marker-only region, awake link).
- **Trigger Conditions**: awake ragdoll link, centre inside a `WaterCurrentVolume` box, no plane surface within its hysteresis band at that XZ.
- **Related**: #5698, #5540, #5697 (angular link damping stand-in).
- **Suggested Fix**: in the marker arm, when `pw.is_multibody_link(body)`, add the same `-c*m*v` force and `-c_ang*m*w` torque the surface arm builds from `b.linear_damping()` / `b.angular_damping()` (without the water coefficients), or let the dry pass run additively for marker-only owners instead of skipping them.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
