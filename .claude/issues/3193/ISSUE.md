# #3193 — SPT-D3-2026-08-20-02: the geometry-tree wind branch added by 6096f19f is unreachable

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3193
- **Labels**: `low,renderer,bug`
- **Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: LOW
**Dimension**: TREE→Billboard Wiring
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md` (`SPT-D3-2026-08-20-02`) — HEAD `bb0b92f2`

## Status

NEW — introduced by **`6096f19f`**, which is the commit that motivated auditing SpeedTree this cycle.

## Location

- `byroredux/src/systems/billboard.rs` — the geometry-tree loop in `make_billboard_system` and its
  `geometry_bases` cache
- `byroredux/src/cell_loader/spawn/mesh_instance.rs` — the mesh-entity `Billboard` + `SpeedTreeWind`
  attach
- `byroredux/src/cell_loader/spawn.rs` — the placement-root `SpeedTreeWind` attach
- `crates/spt/src/import/mod.rs` — `billboard_mode: Some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP)` on the
  placeholder mesh
- `byroredux/src/cell_loader/references/import.rs` — the only `speedtree_wind: Some(..)` producer

## Description

`6096f19f` added a second loop for "full SpeedTree geometry (rather than billboard impostors)",
predicated on entities that carry `SpeedTreeWind` **and** `MeshHandle` but **not** `Billboard`.
**No such entity can be constructed today.**

- `SpeedTreeWind` has exactly **two** production insert sites: `spawn.rs` (the placement root) and
  `spawn/mesh_instance.rs` (the mesh entity). Both read `cached.speedtree_wind`.
- `cached.speedtree_wind` is `Some` at exactly **one** construction site,
  `cell_loader/references/import.rs` — the `.spt` route. Every other `CachedNifImport` constructor
  (`partial.rs`, `precombined.rs`, `references/import.rs`'s non-SPT arm) hard-codes `None`.
- On that route the imported scene has exactly one mesh, and `crates/spt/src/import/mod.rs` gives it
  `billboard_mode: Some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP)`, so `mesh_instance.rs` **always** attaches
  `Billboard` to the same entity it attaches `SpeedTreeWind` to.
- The placement root gets `SpeedTreeWind` but never a `MeshHandle`, so it fails the `mesh_q.contains`
  test.

The branch is therefore dead in every production configuration. Confirmed by
`grep -rn "SpeedTreeWind" byroredux/src crates --include='*.rs'` (two non-test insert sites) and
`grep -rn "speedtree_wind"` (one `Some` producer), plus the `#3076` guard
`placeholder_uses_default_size_without_bounds` which asserts node `billboard_mode == None` / mesh
`billboard_mode == Some(..)` — closing the argument.

### Latent stale-base hazard in its cache

`geometry_bases.entry(entity).or_insert(global.rotation)` snapshots the authored rotation the **first**
frame an entity is seen and never refreshes it. The `retain` prunes only ids that have lost `MeshHandle`
or `SpeedTreeWind`, so an entity id **recycled within a single cell transition** (despawn + respawn
between two runs of this system) inherits the previous tree's base pose. Any later legitimate rewrite of
the entity's `GlobalTransform` (parent motion, structural rebuild) is likewise ignored in favour of the
cached base.

## Impact

Dead code in a hot per-frame system, carrying a cache whose invalidation is wrong for the moment it
becomes reachable. **No present runtime effect.**

Its real cost is diagnostic: the branch's presence reads as *"geometry trees are handled"*, which is the
sort of claim that gets inherited by the next cycle's report — and it is the branch the current wind
model is actually *correct* for (see #3191), which makes the reachable consumer's incorrectness easy to
miss.

## Suggested Fix

Either wire a real producer — Skyrim+ `.nif` trees are the obvious candidate and would make this branch
the *correct* consumer of the current wind model — or **remove the branch until one exists**.

If it stays: key `geometry_bases` on the entity's generation as well as its index, and refresh the base
when the entity's `Transform` is dirty rather than caching for the entity's lifetime.

## Related

- **#3137** (`PERF-D1-02`, OPEN) — **owns the hashing/allocation half of this site**: `geometry_bases` is
  a `std::collections::HashMap` (SipHash) against the **#2923** per-frame `FxHashMap` convention, and its
  `retain` runs unconditionally every frame. Do not duplicate that half here; this issue is the
  **unreachability** and the **stale-base invalidation**.
- **#3192** (`SPT-D2-2026-08-20-02`) — why the `retain` and this loop are paid at all in windy exteriors.
- **#3191** (`SPT-D2-2026-08-20-01`) — the wind model is coherent only for this dead consumer.

## Completeness Checks

- [ ] **SIBLING**: if the branch is removed, the `Billboard`-arm comment claiming the geometry loop is
      what prevents double wind application must be corrected — the root is actually skipped because it
      has no `MeshHandle`
- [ ] **TESTS**: if a producer is wired, a guard pins that a geometry tree entity carries
      `SpeedTreeWind` + `MeshHandle` without `Billboard`; if the branch is removed, no test regresses
      (all current `SpeedTreeWind` tests construct billboards)
