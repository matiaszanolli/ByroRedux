# PHYS-D5-2026-08-20-02: #2549's sensor colliders are excluded only by cast_ray — the non-collidable Havok layer still walls off the player

**Issue**: #3116 — https://github.com/matiaszanolli/ByroRedux/issues/3116
**Finding**: `PHYS-D5-2026-08-20-02`
**Labels**: bug, high, legacy-compat
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_PHYSICS_2026-08-20.md` — Dimension 5 (Character Controller), with Dimension 1 (Shape Translation) and Dimension 7 (Queries & Diagnostics) consequences
**Severity**: HIGH · **Status**: NEW

## History — this was an explicitly *disproved* candidate that has since gone live
`docs/audits/AUDIT_PHYSICS_2026-08-16.md` recorded this verbatim in its **Disproved Candidates** section:

> **`move_character` does not exclude sensors.** … a trigger volume would block the player. **Inert**: nothing in the engine creates a sensor collider … Not filed; worth re-checking the day `TriggerVolume` grows a real Rapier body.

Commit `00fc0f3b` (Fix #2549) added `.sensor(!n.body_data.collidable)` to `register_newcomers`. **The engine now creates sensor colliders**, so the precondition that made that candidate inert is gone — and the KCC / ground-probe filters were not updated with it. This is not a regression of #2549: #2549's parse-side and registration-side halves are both correct. It is the pre-registered conditional hazard firing.

## Location
- `crates/physics/src/sync.rs:851-861` — the producer
- `crates/physics/src/world.rs:1013-1017` — `move_character`
- `crates/physics/src/world.rs:666` — `cast_ray_down`
- `crates/physics/src/world.rs:815` — `cast_capsule_down_surface_and_normal` (shared body of `cast_capsule_down` / `cast_capsule_down_onto_walkable_surface`)
- `crates/physics/src/world.rs:846-872` — `static_colliders_aabb`

## Trigger conditions
Any REFR whose NIF authors a `bhkRigidBody` on Havok layer 15 (`OL_NONCOLLIDABLE` / `FOL_NONCOLLIDABLE` / `SKYL_NONCOLLIDABLE` — the same numeric value across Oblivion / FO3 / FNV / Skyrim LE+SE, per `crates/nif/src/import/collision/mod.rs:240-248`), positioned where the player walks or where a spawn probe lands.

## Description
`register_newcomers` now builds those colliders as Rapier sensors, on the stated grounds that a sensor is "present in the solver, no contact response … and (per `gameplay_ray_ignores_trigger_sensors` in world.rs) already excluded from ray queries elsewhere in this crate". The second half of that claim covers exactly **one** of the five query entry points:

| Entry point | Filter at HEAD | Sees sensors? |
|---|---|---|
| `cast_ray` (gameplay/combat ray) | `QueryFilter::default().exclude_sensors()` | no ✅ |
| `move_character` (the KCC) | `QueryFilter::default()` (+ optional `exclude_collider`) | **yes** ❌ |
| `cast_ray_down` (spawn ground probe) | `QueryFilter::exclude_dynamic().groups(ground_probe_groups())` | **yes** ❌ |
| `cast_capsule_down*` (walkable probe) | same as above | **yes** ❌ |
| `static_colliders_aabb` (world-health census) | iterates all `Fixed`-parented colliders | **yes** ❌ |

Rapier 0.22's `KinematicCharacterController` does not add the flag for you: the only mutation it makes to the caller's filter is `filter.flags |= QueryFilterFlags::EXCLUDE_DYNAMIC` (`rapier3d-0.22.0/src/control/character_controller.rs:670`), and the sweep at `:264-277` passes that same filter straight into `queries.cast_shape`. Nothing in the file references `EXCLUDE_SENSORS` or `is_sensor`. `ground_probe_groups()` (`world.rs:87-90`) is an *interaction-group* mask, not a sensor filter — it only masks out `ACTOR_BONE_GROUP`.

## Evidence
```rust
// world.rs:1013-1017 — the KCC filter
let filter = if let Some(exclude) = params.exclude_collider {
    QueryFilter::default().exclude_collider(exclude)
} else {
    QueryFilter::default()
};
```
vs the sibling that got it right, `world.rs:708`:
```rust
let mut filter = QueryFilter::default().exclude_sensors();
```
Verified at HEAD: `grep -n "exclude_sensors\|ground_probe_groups()" crates/physics/src/world.rs` → `:87` (the fn), `:666` and `:815` (both `exclude_dynamic().groups(...)`, no sensor filter), `:708` (the only `exclude_sensors`).

The two tests that landed with #2549 (`noncollidable_body_registers_as_a_sensor`, `collidable_body_does_not_register_as_a_sensor`, `sync.rs:1121` / `:1157`) assert only `collider.is_sensor()` — neither drives a cast or the controller past one, so the whole consumer half of the change is untested.

## Impact
Three distinct user-visible failures from one gap.

1. **Invisible walls** — the player is blocked by geometry the author marked explicitly non-collidable, which is the exact bug #2549 was filed to fix. Before the fix the body was a *solid* collider; after it, it is a sensor the KCC still treats as solid. For the character controller the change is a no-op.
2. **False floors** — `cast_ray_down` / `cast_capsule_down_onto_walkable_surface` ground the player on a non-solid marker; the player then falls through it on the first step. This is a new member of the door-threshold spawn-gap family and can be mistaken for it. Any future investigation of the door-threshold gap must rule this path out first.
3. **False health signal** — `static_colliders_aabb` counts sensors toward the "collision world is populated" census, the opposite of the discrimination #2874 built into `NearbyCollider::is_sensor` ("a sensor sitting where the floor should be is not a floor", `world.rs:108-110`).

## Related
#2549 (CLOSED, correct as far as it goes), #2874, #2876, `docs/audits/AUDIT_PHYSICS_2026-08-16.md` § Disproved Candidates. Producer half (`havok_filter_is_collidable`, `crates/nif/src/import/collision/mod.rs:246`) is correct — the defect is entirely in the consumer.

## Suggested fix
Add `.exclude_sensors()` to the `move_character` filter and to the shared `QueryFilter::exclude_dynamic().groups(ground_probe_groups())` construction used by `cast_ray_down` and `cast_capsule_down_surface_and_normal` — best done by factoring one `fn solid_probe_filter()` so a fourth cast cannot drift again. Skip `c.is_sensor()` colliders in `static_colliders_aabb`'s count/bounds.

Extend the two #2549 tests to walk a capsule *through* the sensor and to probe *for a floor* beneath one.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every `QueryFilter` construction in `crates/physics/src/world.rs`, not just the four named here
- [ ] **TESTS**: A regression test pins this specific fix (a capsule walk-through and a floor probe, not just `is_sensor()`)
