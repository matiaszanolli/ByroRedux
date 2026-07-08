# #1874 — actual root cause found (differs from the filed hypothesis)

## What the issue filed

The original audit (`docs/audits/AUDIT_RENDERER_2026-07-04.md`, Symptom 2)
narrowed the ghosting to "leading hypothesis H1: a spatially-uniform wrong
motion vector shared by both SVGF and TAA," amplified by TAA's parked-camera
path driving blend weight toward ~99% history. It explicitly did **not**
find the origin of the bad motion vector and recommended a RenderDoc capture
before attempting a fix.

## What I actually found

No RenderDoc available in this environment, but the project had already
shipped a `DBG_VIZ_MOTION` debug-visualization bit (`BYROREDUX_RENDER_DEBUG=0x20000`,
`triangle.frag:424`) explicitly for this issue. Before using it, tracing the
call graph turned up something more direct:

**`cell_loader::transition::reposition_camera` only writes the camera's
`Transform`.** It never touches the player's physics capsule
(`CharacterController` + its kinematic Rapier body). In `PlayerMode::Character`
(the default for any `--cell` boot with a spawned player, i.e. real gameplay),
`camera_follow_system` runs every frame in `Stage::Late` and unconditionally
pins the camera to `body_position + eye_height` — reading whatever the capsule's
current Transform is.

Sequence during a real door-walk transition (`load_interior_cell`):
1. Source cell unloads (`unload_current_interior`) — all its colliders go
   with it.
2. Destination cell loads.
3. `reposition_camera` jumps the **camera** to the destination spawn point.
4. `signal_temporal_discontinuity` fires (correctly) — TAA/SVGF get an
   8-frame recovery window for *this* jump.
5. **Next frame**: `camera_follow_system` reads the capsule's `Transform`,
   which is still sitting wherever it was in the now-unloaded source
   cell — often now ungrounded (its supporting floor collider is gone) and
   free-falling. It snaps the camera back toward that stale/falling position,
   silently undoing step 3.
6. This repeats every frame — a fresh, **unsignaled** camera jump each tick,
   with no matching `signal_temporal_discontinuity` call, for as long as the
   capsule takes to physically settle (which may be many frames, or never,
   if it's falling through space where the old cell used to be).

That recurring, unsignaled fight — not a single bad motion vector — is what
lets the ghost "stick indefinitely" instead of resolving after 8 frames like
every other discontinuity in this engine (streaming, save load, resize all
correctly call `signal_temporal_discontinuity` and, for save/load, also sync
the capsule — see below).

## Confirming evidence

- `grep -rn "reposition_camera" byroredux/src/` found exactly two call
  sites (`cell_loader/transition.rs::load_interior_cell`, and
  `main.rs`'s `TransitionDestination::Exterior` arm) — **neither** touched
  `CharacterController`.
- `save_io.rs`'s save-load-restore path (`restore_player_pose`, ~line 297-337)
  **already implements the correct pattern**: when `PlayerMode::Character`,
  it writes the body's `Transform` + `GlobalTransform`, resets
  `CharacterController` (`vertical_velocity = 0`, `is_grounded = false`,
  `wants_jump = false`), and calls `set_kinematic_translation` to sync the
  Rapier body — only falling back to camera-only `reposition_camera` for
  FlyCam / no-live-body cases (where `camera_follow_system` doesn't run, so
  there's nothing to desync). This is independent confirmation the fix
  pattern is right — it just wasn't applied to the cell-transition path.
- `debug_load.rs`'s three `signal_temporal_discontinuity` call sites (debug
  NIF/cell/exterior-grid loading) don't reposition the camera at all — they
  load geometry into the current view — so they don't need the capsule sync;
  their existing discontinuity signal is correct as-is for the "history no
  longer matches the newly-streamed geometry" case.

## Separately found, NOT fixed here (low-risk dev-tooling gap)

`byro-dbg`'s `cam.tp <entity>` console command (`byroredux/src/commands/view.rs`,
`CamTpCommand::execute`) takes `world: &World` only — it structurally
**cannot** reach `VulkanContext` to call `signal_temporal_discontinuity`,
and obviously doesn't touch the character capsule either. This means any
debug-tool camera teleport (which is how a very similar-looking ghosting
artifact was independently reproduced and reported by the user this session,
in `GSProspectorSaloonInterior`) will still show transient ghosting — that's
expected given the console-command architecture (`&World`-only access, no
route to the renderer), not a bug in the fix above. Confirmed via a clean
A/B test (session changes stashed, rebuilt from unmodified `main`) that this
debug-only symptom is pre-existing and reproduces identically without any of
today's changes. Out of scope for #1874 (which is about real gameplay), but
worth a follow-up ticket if `byro-dbg`-driven visual QA is a priority — would
need either widening `ConsoleCommand::execute`'s signature to accept `&mut
VulkanContext` (touches every command implementation) or a side-channel
resource the main loop polls after console commands run.

## Fix

Extracted the snap-body-to-camera logic already used by
`toggle_player_mode` (Fly → Character) into a shared
`systems::character::snap_character_body_to_camera(world)`, and call it
right after both `reposition_camera` call sites in the transition path.
No-ops harmlessly in FlyCam mode or when there's no player body (matches
existing behavior for those cases elsewhere in the codebase).
