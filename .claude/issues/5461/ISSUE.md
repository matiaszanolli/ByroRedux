# #5461: SCR-D5-2026-10-08-05: The #3817 release doc says the exit-cart path "never reads `vehicle`", but `Effect::ExitCart` reads it for the exit root-motion rotation

**Labels**: low,scripting,documentation,doc-rot,test-gap,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5461

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-10-08.md` — `SCR-D5-2026-10-08-05` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: the doc at `cinematic.rs` ~472 still says the exit path "never reads `vehicle`"; `Effect::ExitCart` (`fragment/effects.rs` ~1540) reads `state.vehicle` + `vehicle_local_rotation`.

- **Severity**: LOW (doc rot plus an unpinned equivalence)
- **Dimension**: Scene/Package/Dialogue (cinematic)
- **Untrusted-Input**: No
- **Location**:
  - `byroredux/src/systems/cinematic.rs:466-472`: the doc.
  - `:545-550`: the release clears `vehicle`.
  - `crates/scripting/src/fragment/effects.rs:1538-1548`: `ExitCart` derives `exit_root_motion_rotation` from
    `state.vehicle` + `vehicle_local_rotation`, falling back to the actor's own rotation.
- **Status**: NEW
- **Description**: after a terminal release, `ExitCart` takes the fallback branch. The result is equivalent only if the
  attachment system left the rider's `Transform.rotation` equal to `vehicle.rotation * local_rotation` on the last
  tethered tick. That is plausible for a parked cart, but nothing pins it, and the doc asserts the opposite of what the
  code does.
- **Impact**: none observed. A later change to the attachment cadence would silently change the MQ101 exit heading.
- **Suggested Fix**: correct the doc, and add a lifecycle test asserting `exit_root_motion_rotation` is unchanged across
  a terminal release, or have the release snapshot the composed rotation.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other release-path doc claims in `systems/cinematic.rs`)
- [ ] **TESTS**: A regression test pins this specific fix
