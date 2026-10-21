# #6325: PHYS-D2-2026-10-21-02: Which bodies emit contact-force events depends on when the subscription toggle flipped

**Labels**: low,bug,physics

**Source**: docs/audits/AUDIT_PHYSICS_2026-10-21.md · **Severity**: LOW · **Dimension**: Step & Sync · **Location**: `crates/physics/src/world/mod.rs:734-768` (`set_contact_force_events_enabled`), `crates/physics/src/sync.rs:1155-1172` (registration rule), `crates/physics/src/ragdoll.rs:446-457` (no `active_events`), `world/mod.rs:1236-1242` (`set_motion_type` → `dynamic_bodies`)

- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/world/mod.rs:734-768` (`set_contact_force_events_enabled`), `crates/physics/src/sync.rs:1155-1172` (registration rule), `crates/physics/src/ragdoll.rs:446-457` (no `active_events`), `world/mod.rs:1236-1242` (`set_motion_type` → `dynamic_bodies`)
- **Status**: NEW (#5711 / `e3a4d5e18` follow-up)
- **Trigger Conditions**: `ImpactConfig` gains sound content. When it does, ragdolls and script-flipped dynamics exist at different times relative to the toggle.
- **Description**: The registration rule tags a collider only if its recipe is `MotionType::Dynamic`. The toggle instead retags every collider whose parent is in `dynamic_bodies`. That index also holds ragdoll links (`ragdoll.rs:408`) and bodies flipped to Dynamic by `set_motion_type`. So a ragdoll or flipped body that exists on the toggle frame starts emitting impacts. One built or flipped a frame later never does, because `build_ragdoll` and `set_motion_type` set no `active_events`. Disabling and then re-enabling changes the set again.
- **Evidence**: `world/mod.rs:740-755`; `ragdoll.rs:446-457`.
- **Impact**: Dormant today (`ImpactConfig` ships without content, `systems/audio.rs:372-377`). Once content lands, corpse-bone and scripted-body impact sounds will be inconsistent and depend on history.
- **Related**: #5711, #5557 (ragdoll colliders carry no surface material either).
- **Suggested Fix**: Pick one population (recipe-Dynamic, or `dynamic_bodies`) and apply it in all three producers. That means retagging in `set_motion_type` and having `build_ragdoll` read `contact_force_events_enabled`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_PHYSICS_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
