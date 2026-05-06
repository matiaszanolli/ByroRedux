## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
MEDIUM / ECS Lifecycle

## Location
`byroredux/src/main.rs:315`, `byroredux/src/main.rs:340`, `byroredux/src/systems.rs:773-843`

## Description
`footstep_system` is registered in `Stage::Update` (main.rs:315), running BEFORE `transform_propagation_system` in `Stage::PostUpdate` (main.rs:321). The footstep system reads `GlobalTransform.translation` to generate the world-space dispatch position (systems.rs:799-803). The acknowledgement comment (main.rs:305-314) admits the GlobalTransform is "one frame stale relative to the camera's Transform" but waves it off as "~3 cm of motion." For a fly-cam at 200+ engine units/s (sprint + boost), that's 200/60 = ~3.3 units/frame — not 3 cm but 3 game units. At Bethesda interior scales (cells are ~50-200 units across) that's noticeable spatial-pan offset. The audit dimension explicitly flags this: "running before transform propagation reads stale GlobalTransform for the listener and emitters."

## Evidence
```
# byroredux/src/main.rs:305-315
// M44 Phase 3.5: footstep dispatch. Runs in Stage::Update so
// it sees the post-fly-camera Transform (which the camera
// system writes in Stage::Early) but BEFORE
// transform_propagation in Stage::PostUpdate — so the
// GlobalTransform we read is one frame stale relative to the
// camera's Transform. That's acceptable for footstep position
// accuracy at human movement speeds (~1 frame at 60 FPS = 17
// ms = ~3 cm of motion).
scheduler.add_to(Stage::Update, footstep_system);
...
scheduler.add_to(Stage::PostUpdate, make_transform_propagation_system());
```
`audio_system` itself runs at `Stage::Late` (main.rs:340) — that part is correct (verified against the audit checklist at audit-audio.md:112). The issue is the upstream queue producer's stage.

## Impact
Footsteps at fast-travel speeds (and during teleports, warp-debug commands) audibly trail the listener. For human-walk speeds (~5 units/s), the comment's 3-cm claim is roughly right; for sprint (~30+ units/s) it's 0.5 units; for fly-cam boost it's 3+ units. Bethesda's "feels like a game" axis cares about footstep-to-position correlation.

## Suggested Fix
Move `footstep_system` to `Stage::PostUpdate` AFTER `transform_propagation_system`, OR have it read `Transform` directly when the entity has no parent (then the local transform IS the world transform). The comment's reasoning ("3 cm of motion") underestimates the worst case by ~100×. Pin a regression test that spawns a fast-moving entity, ticks one frame, and asserts the emitted footstep position matches the post-propagation GlobalTransform.

## Related
M44 Phase 3.5b (FOOT records).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
