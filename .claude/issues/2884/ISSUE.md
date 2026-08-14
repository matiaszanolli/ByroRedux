# PHYS-D4-04

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2884

---

Found by `/audit-physics` Dimension 4 (Ragdoll Articulation — coverage). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/ragdoll.rs:475-485` (`remove_ragdoll`), `:160-162` (damping add), `crates/physics/src/config.rs:84-89` + `:131-136`

## Trigger Conditions
A Rapier upgrade or a `remove_body`/`build_ragdoll` refactor that reintroduces the #1531 leak shape, or a stray edit that makes `ragdoll_extra_angular_damping` non-zero by default or applies it per-constraint. **Both would ship green.**

## Description
`grep -rn "remove_ragdoll" byroredux crates` yields exactly three hits — the definition, one doc reference (`components.rs:159`), and one production call site (`byroredux/src/cell_loader/unload.rs:477`). **Zero tests.** The one adjacent test, `reactivating_ragdoll_does_not_leak_previous_bodies` (`byroredux/src/ragdoll.rs:886-981`), covers the #2083 double-*activate* path, not the build->remove cycle that #1531 was filed against.

Separately, `default_contact_config_matches_previous_inline_values` (`config.rs:131-136`) asserts `kcc_offset_bu` and `default_contact_skin_bu` but **not** `ragdoll_extra_angular_damping`, and no test asserts the addition happens once per **body** rather than once per constraint.

## Evidence
The behaviour itself is currently **correct** — verified with a throwaway probe (7-body branching tree, 5 build->step->remove cycles, deleted after the run):

```
cycle 0 live = (7, 7, 1)   cycle 0 after remove: bodies=0 colliders=0 multibodies=0 awake=(0, 0)
... cycle 4 live = (7, 7, 1) cycle 4 after remove: bodies=0 colliders=0 multibodies=0 awake=(0, 0)
```

Confirmed against Rapier 0.22 source: `RigidBodySet::remove` (`rigid_body_set.rs:112-113`) calls both `impulse_joints.remove_joints_attached_to_rigid_body` and `multibody_joints.remove_joints_attached_to_rigid_body`, and cascades colliders via `remove_attached_colliders = true`. Damping: applied once in the body loop at `ragdoll.rs:160-162`, absent from the joint loop at `:217-261`; default is `0.0` at `config.rs:88`.

## Impact
Coverage only — nothing is broken today. But #1531 (ragdoll leak on cell unload) is exactly the regression this gap would hide, and the damping dial is called out in `docs/engine/physal.md` §4 as *"the biggest 'less floppy than Havok' lever"*, so a silent default change would alter every ragdoll's feel with no test failure.

## Suggested Fix
Promote the probe into `crates/physics/src/ragdoll.rs`'s test module — build a **branching** (not linear) spec, step, `remove_ragdoll`, assert `body_count() == 0` / `colliders.len() == 0` / `multibody_joints.multibodies().count() == 0`, repeated 3x to catch arena drift. Add `assert_eq!(c.ragdoll_extra_angular_damping, 0.0)` to `default_contact_config_matches_previous_inline_values`, plus a test that a non-zero config yields `authored + extra` on each body's `angular_damping` for a 2-body/1-joint spec.

## Related
- #1531, #2083, #1520; `docs/engine/physal.md` §4
