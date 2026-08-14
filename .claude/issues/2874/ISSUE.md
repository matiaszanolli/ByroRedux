# PHYS-D7-03

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2874

---

Found by `/audit-physics` Dimension 7 (Queries & Diagnostics). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/sync.rs:375-476` (`dump_spawn_collider_census`, its doc block, and the summary `log::warn!` at `:445-454`); `byroredux/src/scene.rs:1113-1128` (its only call site); `crates/physics/src/world.rs:694-727` (`cast_capsule_down_surface_and_normal`, private)

## Trigger Conditions
A door-teleport spawn whose floor probe misses all three rungs (`floor_probe_failed == true`, `scene.rs:1120`). Every such run produces a census that under-determines the cause.

## Description
The census's own doc block (`sync.rs:380-393`) enumerates four candidate causes and the summary log line (`:445-454`) maps them onto observable tallies. Two of the three cases this diagnostic exists to separate are not actually separable by that mapping:

**(a) no collider authored vs (b) collider dropped in translation** — both land on the same bucket. The log says *"0 total => the collider never spawned (per-NIF trimesh-fallback gate, or a REFR-level gap)"*; that single sentence **is** the conflation. The engine already computes the discriminator elsewhere and the census never consults it: `summarize_collision_authoring` / `CollisionAuthoringSummary` (`crates/nif/src/import/collision/mod.rs`, retained on `CachedNifImport`) carries the classic / new-physics / phantom counts, and `docs/engine/physics.md:330-338` states its whole purpose is that *"an empty decoded-collision array no longer conflates 'intentionally no collision' with 'packed collision exists but is undecodable'"*. The census reads the Rapier side only, so it **re-introduces exactly the conflation that summary was built to remove**.

**(c) collider present but not walkable** — invisible. All three spawn rungs call `cast_capsule_down_onto_walkable_surface`, which returns `None` for *both* "swept capsule hit nothing" and "swept capsule hit something whose `normal1.y` failed the walkable test" (`world.rs:675-692`). The normal is computed and then discarded: the surface-and-normal helper is private and only `cast_capsule_down` (surface only) is public.

So a spawn that is blocked by a 60-degree ramp logs *"MISS on all 3 rungs"* and then a census showing `Fixed>0` — and the summary line instructs the reader to conclude *"Fixed>0 at a wrong Y => transform composition"*. **The diagnostic actively mis-attributes a walkability rejection to a transform bug.**

## Evidence
- `sync.rs:448-451` — the summary text: three arms, no walkability arm, no authoring-summary arm
- `world.rs:689-691` — `.and_then(|(surface_y, normal_y)| (...).then_some(...))`, the `None` return collapsing miss and reject
- `world.rs:694` — `fn cast_capsule_down_surface_and_normal` is private, so no caller can recover the normal
- `sync.rs:343-353` — `SpawnCensusEntry` has no walkable/normal field and no authoring field

## Impact
The one diagnostic built for "why is there no floor here" leaves the operator with two of the three real causes indistinguishable, and steers them toward the wrong one in the third. This is the observability layer under a defect class that has already consumed #1295, #2013 and #2202 — and, per this audit, PHYS-D5-01 / D5-02 / D5-03.

## Suggested Fix
1. Make `cast_capsule_down_surface_and_normal` `pub`, and on the failure path re-run it unfiltered so the log can say *"unfiltered sweep hit y=... normal_y=... -> REJECTED as non-walkable (min=...)"* versus *"no hit"*.
2. Add the cell's `CollisionAuthoringSummary` totals to the census header so `0 total` splits into "nothing authored" vs "N classic / M new-physics authored, none registered".

## Related
- PHYS-D7-04 (same function, different defect), PHYS-D7-05 (unreachable at runtime — worth fixing in the same change)
- #2202 (the issue that created the census)
