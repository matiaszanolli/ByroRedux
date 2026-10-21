# #6294: SAVE-2026-10-21-04: #6085's `NOT_PARKED` partition still has nine `UNAUDITED` rows; two of them revert player-visible state on every cell exit

**Labels**: low,save-load,test-gap,bug

**Source**: docs/audits/AUDIT_SAVE_2026-10-21.md · **Severity**: LOW · **Dimension**: Snapshot Completeness & the Two Lists (third list) · **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:1176-1197`

- **Data-Loss Class**: silent-drop on eviction (save/load is unaffected)
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:1176-1197`
- **Status**: NEW (follow-up to closed #6085, which seeded the rows)
- **Description**: The partition guard accepts `"UNAUDITED"` as a reason, so these rows stay green indefinitely. Proposed dispositions, from reading their mutators:
  - **CharacterLevel / LevelUpPoints**: player progression, and the player is never evicted. The reason should say so, as `CharacterController`'s row does.
  - **TimedRestorations**: the only production insert is on the player (`inventory.rs:1567`). Same reason.
  - **LightSource / LightFlicker**: mutated only by `light_anim` flicker playback, which is re-derived. Not parked is correct.
  - **GuardState / PatrolState**: AI runtime that re-rolls on respawn, the same reason `WanderState` carries.
  - **Escorted**: a terminal marker on a finished escort. After eviction the escort package can run again and fire its PF_ fragments a second time. Needs a park-or-justify decision (gameplay-owned).
  - **RigidBodyData**: a scripted `SetMotionType` (`cinematic.rs:436-445`) reverts on cell exit. It is moot until SAVE-2026-10-21-01 moves motion overrides to a FormID ledger, which would then cover eviction as well.
- **Impact**: low. The Escorted and RigidBodyData rows are the only ones with gameplay effect. Leaving `UNAUDITED` as a valid reason defeats the guard's purpose.
- **Related**: #6085, GAME-SD-01/02, SAVE-2026-10-21-01.
- **Suggested Fix**: write the reasons above. Make the guard reject `"UNAUDITED"` once they are written, or at least fail if the count grows.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
