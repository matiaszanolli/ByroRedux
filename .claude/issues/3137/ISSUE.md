# PERF-D1-02: the delta's new per-frame water and vegetation collections landed on std SipHash

**Issue**: #3137 — https://github.com/matiaszanolli/ByroRedux/issues/3137
**Labels**: `low,performance,bug`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md`

---

**Severity**: LOW
**Dimension**: CPU Per-Frame Allocations & Hot Paths
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md` (PERF-D1-02)

## Location

- `byroredux/src/systems/water.rs:10, 356, 367-369, 435` (`make_water_interaction_system`)
- `byroredux/src/systems/billboard.rs:9, 36, 144, 153` (`make_billboard_system`'s `geometry_bases`)

## Status

NEW — both sites landed in this delta (`5959bbb8`, 2026-08-19; `6096f19f`, 2026-08-20).

## Description

`_audit-common.md`'s hot-path hashing rule (#2923) requires the per-frame render/skinning path to be `FxHashMap`/`FxHashSet` end-to-end. **The guard it names is intact** — `pose_dirty_crosses_the_crate_boundary_without_siphash` (`crates/renderer/src/vulkan/context/mod.rs:4402-4423`), and every `SkinSlotPool` collection, `FrameInputs.pose_dirty` and the `skin_offsets` map in `byroredux/src/render/` is still Fx. **The two collections added this session did not follow the rule.**

| Site | Collection | Per-frame behaviour |
|---|---|---|
| `systems/water.rs:356/367` | `wet_last_frame` / `wet_now`: `std HashSet<EntityId>` | `wet_now` is built fresh each frame, then `wet_last_frame = wet_now` (`:435`) — the prior set's capacity is dropped, so a scene with wet bodies regrows 0→N **every frame**. This is the `mem::take` capacity-churn shape #1371 fixed for `PackedStorage`. |
| `systems/water.rs:369` | `ripple_by_surface`: `std HashMap<EntityId, …>` | fresh `HashMap::new()` per frame |
| `systems/water.rs:368` | `entries: Vec::new()` | fresh per frame |
| `systems/billboard.rs:36` | `geometry_bases: std HashMap<u32, Quat>` | one SipHashed `.entry()` per SpeedTree geometry entity per frame, **plus** an unconditional `retain` (`:153`) that walks the whole map and issues two sparse-set `contains` probes per live tree, every frame |

## Evidence

Confirmed at HEAD:
```
byroredux/src/systems/water.rs:10:use std::collections::{HashMap, HashSet};
byroredux/src/systems/water.rs:356:    let mut wet_last_frame = HashSet::<EntityId>::new();
byroredux/src/systems/water.rs:369:        let mut ripple_by_surface = HashMap::<EntityId, (EntityId, Vec3, f32)>::new();
byroredux/src/systems/water.rs:437:        wet_last_frame = wet_now;
byroredux/src/systems/billboard.rs:9:use std::collections::HashMap;
byroredux/src/systems/billboard.rs:36:    let mut geometry_bases: HashMap<u32, Quat> = HashMap::new();
byroredux/src/systems/billboard.rs:153:            geometry_bases.retain(|entity, _| mesh_q.contains(*entity) && swq.contains(*entity));
```

Both crates already depend on `rustc-hash`.

**The billboard `retain` is not gated in practice.** `billboard.rs`'s camera-motion gate (#1374, `:90`) is `last_cam == … && !wind_active && !wind_state_changed`, and `wind_active` is true whenever `WindField.speed > 1.0e-4` — so in any weathered exterior the gate never fires and the `retain` is paid unconditionally.

## Impact

Small in absolute terms and bounded by live-tree / wet-body counts, and neither set is DoS-facing. The cost is (a) SipHash on a per-frame per-entity keyspace, (b) an allocation-per-frame shape in the water set, and (c) an O(live trees) prune that only ever needs to run on despawn.

**The larger cost is epistemic**, exactly as #3061 argued: the #2923 guard makes the per-frame path *read* as Fx-hashed while newly added siblings are not, so the next auditor (or the next author copying a neighbouring system) inherits the wrong pattern.

## Suggested Fix

- Substitute `FxHashMap` / `FxHashSet` at all four sites.
- In `make_water_interaction_system`, **swap** the two sets rather than move-assigning, so capacity survives: `std::mem::swap(&mut wet_last_frame, &mut wet_now); wet_now.clear();`
- Hoist `entries` / `ripple_by_surface` into closure-captured scratch reused via clear+extend.
- Drive `geometry_bases` pruning off cell unload — or run the `retain` only when the map's length exceeds the live `SpeedTreeWind` count — instead of every frame.

## Related

- #3061 (`PERF-D6-01`, OPEN) — the renderer-side siblings, explicitly scoped to the skinning path and therefore **not** covering these
- #3045 (`REN-D9-01`, OPEN)
- #1374 (the billboard camera-motion gate — intact, but bypassed by wind)
- #2923 (CLOSED — the rule these sites missed), #1371 (the capacity-churn precedent)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — sweep `byroredux/src/systems/` for any other `use std::collections::HashMap` on a per-frame path added this session
- [ ] **TESTS**: A regression test pins this specific fix (extend the #2923-style source-level guard to cover `byroredux/src/systems/`, so the next per-frame map cannot land on SipHash unnoticed)
