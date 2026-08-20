# Issue #3118: WaterMaterial grew 18 -> 63 fields this delta, and WaterContact copies all 436 bytes of it into per-body ECS storage every physics tick

- **Finding ID**: `ECS-2026-08-20-07`
- **Severity**: LOW
- **Labels**: `low,ecs,bug`
- **Source report**: `docs/audits/AUDIT_ECS_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3118

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3118 --json state`.

---

**Severity**: LOW
**Dimension**: 8 — Hot-Path Performance Invariants
**Source**: `docs/audits/AUDIT_ECS_2026-08-20.md` (`ECS-2026-08-20-07`)

**Location**
- `crates/core/src/ecs/components/water.rs:88-368` (`WaterMaterial`), `:564-594` (`WaterContact`)
- `crates/physics/src/water.rs:581`, `:724-760`, `:802-808`

## Description

The WATAL authored-parameter work took `WaterMaterial` from **18 fields at `adbc3f77` to 63 at HEAD** —
436 bytes by field sum, all `Copy`. (Counts measured directly against both revisions.)

`WaterContact` embeds `Option<WaterMaterial>` and `Option<WaterFlow>`, so each per-body contact is now
roughly half a kilobyte, and the buoyancy phase rebuilds and re-inserts one for **every wet dynamic body,
every physics tick**, through a freshly-allocated `Vec<(EntityId, WaterContact)>`.

## Evidence

`crates/physics/src/water.rs:581`:

```rust
let mut writes: Vec<(EntityId, WaterContact)> = Vec::new();
```

then `:802-807` drains it into `query_mut::<WaterContact>()` one insert at a time:

```rust
match world.query_mut::<WaterContact>() {
    Some(mut wq) => {
        for (entity, contact) in writes {
            wq.insert(entity, contact);
        }
    }
```

`SubmersionState` (`crates/core/src/ecs/components/water.rs:531-543`) carries the same
`Option<WaterMaterial>` and is whole-struct assigned each frame (`byroredux/src/systems/water.rs:334`) —
one entity, so negligible there; the per-body path is the one that scales.

## Impact

Modest today (a few dozen floating bodies => low tens of KB of memcpy per tick, plus one `Vec` growth),
and `Vec::new()` costs nothing on the dry-scene fast path.

Flagged because it is a ~3.5x field growth in a per-body, per-tick component that landed **without the
scratch-reuse treatment every other per-frame buffer in this codebase got** (#932 `FootstepScratch`,
#3059 `InteractionCandidateScratch`, #828 `animation_system`), and because the duplicated payload is
already reachable through `WaterContact::surface_entity` -> `WaterPlane::material`.

## Related

- #932, #3059 (the scratch-reuse precedent)
- #2887 (`WaterContact::depth` measurement — same component, different defect; not a duplicate: no prior
  issue names `WaterContact`'s size)

## Suggested Fix

Either:

- hold the surface's material **by reference** through `surface_entity` (the field already exists and
  every consumer has `World`), or
- keep the embedded copy but hoist `writes` into a scratch `Resource` the way `FootstepScratch` does, so
  the capacity survives across ticks.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — `SubmersionState`'s embedded
      `Option<WaterMaterial>`, and any other per-tick `Vec` allocated inside the buoyancy phase
- [ ] **TESTS**: A regression test pins this specific fix — e.g. assert the scratch `Vec`'s capacity
      survives across two ticks, in the shape #932's `FootstepScratch` test uses
