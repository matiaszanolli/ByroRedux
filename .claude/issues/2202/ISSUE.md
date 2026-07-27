title:	SKY-2026-07-26-01: BleakFallsBarrow01 entrance vestibule has no floor collider — player free-falls from door spawn (black screen, all assets loaded)
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, high, import-pipeline, legacy-compat
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2202
--
**Severity**: HIGH
**Dimension**: TES-family grounding chain / collision import (found live on real Skyrim SE data, 2026-07-26)
**Location**: mechanism not isolated — candidates at `crates/nif/src/import/collision/mod.rs:303` (`extract_from_classic` reclassification gate), `crates/nif/src/import/collision/shape.rs:78` (`resolve_shape_inner` → `None` arms), and `byroredux/src/cell_loader/spawn.rs:1381` (synthesized-trimesh fallback gate). Symptom consumed at `byroredux/src/scene.rs:648-681` (door-spawn floor ladder) and `byroredux/src/systems/character.rs:195,219`.

## Summary

Skyrim SE's `BleakFallsBarrow01` entrance vestibule has **no fixed collider anywhere beneath the door spawn**, while the same cell reports **2560 fixed colliders elsewhere**. The player capsule spawns at door height and free-falls out of the world with every asset loaded and every texture resolved (black screen, 0 draws, `tex.missing=0`).

This is the follow-up `a88eab6e` said it would file. That commit fixed two real defects in the `#2013` door-spawn probe ladder, and explicitly did **not** claim to fix this cell — the ladder is now correct and the cell is still broken, which is what localises the defect to the collision-import side.

## Why this is filed separately from #2013 and #2193

Three sibling symptoms, all in the TES-family grounding chain, all plausibly one root cause family — "collision geometry survives parse but arrives wrong":

| # | Game | Symptom | State |
|---|---|---|---|
| #1832 | Skyrim SE | architecture ships `mass=0` Dynamic-family bodies → reclassified Static (19 → 416 colliders) | CLOSED |
| #2193 | Oblivion | rests on solid floor, but contact normal inverted (`dot-up ≈ −0.99`) → `is_grounded` stuck false | OPEN, HIGH |
| **this** | Skyrim SE | **no collider at all** beneath the spawn → free-fall | filing now |

`#2013` (the spawn-positioning issue) is CLOSED and should stay closed — its reported symptom (infinite freefall attributable to a bad spawn *position*) is genuinely fixed, and its ROADMAP Known-Issues entry documents the Skyrim half as grounding at frame 0, which is true of `WhiterunDragonsreach`. This is a different subsystem reached through the same symptom.

## Evidence

All from a real Vulkan device + Steam Skyrim SE install, headless bench with `RUST_LOG=info`.

**1. The floor ladder misses all three rungs.** Its own log line (`byroredux/src/scene.rs:713-724`, which `a88eab6e` extended to name the answering rung precisely so a bad spawn identifies its own cause in one run):

```
M28.5 spawn at door teleporter: door at (-7537.2, 512.0, -1018.5);
inward nudge (55.1, _, 32.5) BU; floor probe MISS on all 3 rungs (used door height)
```

That is after `a88eab6e`, i.e. with all three rungs working:
- rung 1 — nudged XZ near door height: miss
- rung 2 — **un-nudged door XZ** near door height: miss (doors sit at floor level by construction; the threshold is solid on every other cell tested)
- rung 3 — full-cell vertical sweep at nudged XZ, `(max.y − min.y) + 100` = **7432 BU**: miss

**2. The cell is not collider-less.** `static_colliders_aabb` (`crates/physics/src/world.rs:575`, counting only `RigidBodyType::Fixed`) reports **2560 fixed colliders** with a Y extent reaching **y = −4514** — so the collision world is populated, correctly scaled, and geographically overlapping the cell. It just has nothing under the vestibule.

**3. Not a penetrating-start artifact.** `cast_capsule_down` (`crates/physics/src/world.rs:539-569`) passes `stop_at_penetration: false`, so a capsule starting inside geometry does not swallow the hit and report a miss.

**4. Not a query-pipeline ordering bug.** The identical probe, at the identical point in cell load, answers on `WhiterunDragonsreach` (`"floor probe hit y=-296.3 via door XZ near door height"`). The QBVH is populated at probe time; the defect is cell/content-specific, not lifecycle.

**5. Not the spawn ladder.** Two independent XZ columns, each swept across the cell's full 7432 BU vertical extent, both empty.

## Candidate mechanisms

Not isolated — each is verified *possible* against current code, none confirmed. Listed with what would discriminate it.

**(A) Double-gap: partial bhk resolve suppresses the trimesh fallback.**
The synthesized-trimesh fallback is gated on `collisions_empty` (`spawn.rs:1381`), which is `cached.collisions.is_empty()` — computed **per NIF, not per shape** (`spawn.rs:331`). `resolve_shape_inner` has many deliberate `None` arms (degenerate radius, over-deep chain, self-cycle, phantom/trigger volumes at `shape.rs:288-303`), and its own comments repeatedly say *"→ `None` → trimesh fallback fires instead."* That reasoning holds only when **every** shape in the NIF resolves to `None`. A vestibule NIF whose floor shape drops to `None` while any sibling shape resolves leaves `collisions` non-empty → fallback suppressed → **no floor, and no fallback either**. This is the mechanism the current gate structurally cannot catch.

**(B) The vestibule floor is Dynamic, not Fixed.** `#1832`'s reclassification fires only for `motion_type == MotionType::Dynamic && body.mass <= 0.0` (`mod.rs:303`). Skyrim architecture authoring `motionType` 1..=5 with **non-zero** mass stays genuinely `Dynamic` — invisible to the `RigidBodyType::Fixed` census in (2) above *and* excluded from the probe by `QueryFilter::exclude_dynamic()` (`world.rs:550`). Note `havok_motion_type` maps raw 6 → `Keyframed` → kinematic, which the probe *does* see, so this narrows to the Dynamic family specifically.

**(C) Collider exists as Fixed but mis-transformed.** The vestibule floor is among the 2560, just composed to the wrong place — a `GlobalTransform::compose_trs` / `havok_scale` error on this shape kind (`spawn.rs:674`).

**(D) The vestibule architecture REFRs never spawn.** Not a collision bug at all but a REFR-level gap; least likely given the cell renders its geometry, but not excluded by anything measured.

## Discriminating diagnostic

One census would separate all four, and none of the existing logs provide it: **at spawn time, dump every collider within some XZ radius of the door, grouped by Rapier body type, with source `PhysicsSourceForm`.** `PhysicsSourceForm` is already attached for exactly this purpose (`spawn.rs:721`, added by `#1698` to resolve a physics proxy back to its placement).

- colliders present, all Dynamic → **(B)**
- colliders present, Fixed, wrong Y → **(C)**
- no colliders near the XZ but the owning REFR resolves → **(A)**
- no owning REFR at all → **(D)**

`static_colliders_aabb` gives cell-wide bounds only, which is why 2560-with-a-hole reads as healthy today.

## Impact

`is_grounded` is not cosmetic. It gates `jump_fired` outright and forces the full gravity-integration branch (`byroredux/src/systems/character.rs:195,219`). A cell where the spawn has no floor is unplayable from frame 0 — and it fails silently in the most misleading way available: every asset loads, every texture resolves, the bench reports healthy entity counts, and the screen is black because the camera is in free fall below the world. That cost ~an hour of renderer investigation before the spawn log identified it.

`BleakFallsBarrow01` is also not an obscure cell — it is Skyrim's first-hour tutorial dungeon, and `docs/smoke-tests/m47-triggers.sh:27` already names it as the recommended trigger-heavy smoke target.

## Repro

```bash
cargo run --release -- --esm Skyrim.esm --cell BleakFallsBarrow01 \
  --bsa "Skyrim - Meshes0.bsa" --textures-bsa "Skyrim - Textures0.bsa" \
  --bench-frames 120 --bench-hold
```
(`$BYROREDUX_SKYRIM_DATA` = `.../Skyrim Special Edition/Data`.) With `RUST_LOG=info`, look for `floor probe MISS on all 3 rungs` followed by `M28.5 static collider AABB: … (2560 fixed colliders)` and a monotonically falling body Y in the per-frame `M28.5 frame N` lines.

Control (must keep working — the non-regression gate for any fix here): `--cell WhiterunDragonsreach` grounds `grounded=true` at frame 0 via rung 2.

## Suggested first step

Land the (A)/(B)/(C)/(D) census as a `log::debug!` on the spawn path before touching any import code. Per the project's no-guessing rule, a winding/classification/gate change made against four live hypotheses risks regressing correctly-oriented floors elsewhere — which is exactly the reasoning that stopped #2193's inverted-normal fix from being guessed at, and the same reason `a88eab6e` shipped the rung-attribution log before proposing anything.

Worth noting for sequencing: if the cause turns out to be **(A)**, the fix is per-shape fallback granularity rather than a per-NIF boolean, and that is a change to the gate every game's static architecture flows through — it wants its own bench + the FO4/Starfield trimesh-fallback smoke cells (`MedTekResearch01`, Cydonia) as regression cover, not just the two Skyrim cells above.

