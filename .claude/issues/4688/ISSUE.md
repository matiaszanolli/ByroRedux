# PHYS-D2-2026-09-21-03: sync.rs module doc points the #2880 "PhysicsWorld is inserted unconditionally" claim at the wrong file

**Issue**: #4688
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: LOW
**Dimension**: Step & Sync
**Location**: `crates/physics/src/sync.rs:26-29`

## Description
`db70595e3` mechanically repointed the old `boot.rs` reference to `byroredux/src/boot/schedule/`, which only registers the system. The one production insertion is `byroredux/src/boot/world.rs:165`.

## Evidence
`grep -n "PhysicsWorld::new()\|insert_resource(PhysicsWorld" byroredux/src/boot/` finds exactly one production hit at `boot/world.rs:165`; every other hit is a `#[cfg(test)]` fixture.

## Impact
Doc-rot on the claim that closed #2880: a reader following the pointer lands on the registration, not the insertion.

## Related
#2880, #4412 batch (`db70595e3`).

## Suggested Fix
Point at `byroredux/src/boot/world.rs`.
