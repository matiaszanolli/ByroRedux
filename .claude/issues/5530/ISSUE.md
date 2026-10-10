# #5530: PHYS-D2-2026-10-09-02: `accept_keyframe_target` gates only `push_kinematic`; newcomer registration and `set_kinematic_translation` place Rapier bodies at unchecked translations

**Labels**: bug, low, physics

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-09.md` — finding `PHYS-D2-2026-10-09-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. A hardening gap in the same panic class. No content evidence: the field panic did not come
  through these entries.
- **Dimension**: Step & Sync (streamed-collider insert)
- **Location**:
  - `crates/physics/src/sync.rs:1066`: `register_newcomers`, `.position(iso_from_trs(n.global.translation, …))`.
  - `crates/physics/src/sync.rs:87-111`: `set_kinematic_translation`. Its callers are
    `byroredux/src/save_io.rs:781` (`apply_player_pose`, a save-file pose with no finiteness or range check),
    `byroredux/src/systems/character.rs:635,907,1036` and `byroredux/src/commands/view.rs:482`
    (`combat.approach`).
  - Compare `sync.rs:1243-1255`: the only gate, `accept_keyframe_target`, at `recovery.rs:227-254`.
- **Status**: NEW
- **Trigger Conditions**: any of the following:
  - a streamed entity whose `GlobalTransform` at first registration is finite but beyond the multi-SAP saturation
    window (from about 2.1e9 BU for small colliders up to 2.68e11 BU for bone-sized ones), for example a broken
    first animation sample on a live actor's bone, or a corrupt placement;
  - a save whose player pose is corrupt;
  - a debug teleport to such a coordinate.
- **Description**: #5161 refused insane keyframe targets because "the substep recovery only snapshots `Dynamic`
  bodies, and live actor bones are keyframed — this boundary check is the only guard they have". Two other entry
  points put a body into the broad phase at an arbitrary position:
  - **Newcomer registration** builds every streamed body, keyframed bones included, at
    `n.global.translation` with no check. The body is never in the snapshot; a fixed or keyframed one never will
    be.
  - **`set_kinematic_translation`** drives the player capsule. `apply_player_pose` passes it the saved position
    verbatim.

  NaN is harmless here, because rapier rejects non-finite AABBs. A finite out-of-range value is the panic
  precondition analysed in D2-01.
- **Evidence**: `sync.rs:1063-1070` builds the body straight from the newcomer's transform. `save_io.rs:720-781`
  performs no check between `Vec3::from_array(pose.position)` and `set_kinematic_translation`.
- **Impact**: a corrupt transform or save crashes the process instead of being refused or parked. Low
  likelihood; same blast radius as D2-01.
- **Related**: PHYS-D2-2026-10-09-01, whose broad-phase withholding fix also closes both entries; #5161.
- **Suggested Fix**: apply the `accept_keyframe_target` bound in `register_newcomers` (skip the entity and log
  once) and in `set_kinematic_translation` (refuse and return `false`). `apply_player_pose` should reject a
  non-finite or out-of-range pose and fall back to the cell's spawn point. Alternatively, rely on D2-01's
  broad-phase filter as the single backstop.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
