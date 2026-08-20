# #3192 — SPT-D2-2026-08-20-02: regression of #1374 (73896726) — the camera-parked early-out is bypassed every frame in any windy exterior

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3192
- **Labels**: `medium,performance,bug`
- **Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: MEDIUM
**Dimension**: Placeholder Fallback (secondary: TREE→Billboard Wiring)
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md` (`SPT-D2-2026-08-20-02`) — HEAD `bb0b92f2`

## Status

**NEW — regression of #1374's guarantee.** Introduced by **`73896726`**, broadened by **`0304538c`**.
Regression baseline: `byroredux/src/systems/billboard.rs` as of **`73896726~1`**.

## Location

- `byroredux/src/systems/billboard.rs` — the `#1374` camera-motion gate and the `Billboard` loop in
  `make_billboard_system`
- `byroredux/src/scene/world_setup.rs` — `install_ground_cover` (the unconditional exterior `WindField`
  install)

## Description

The **#1374** gate existed to stop `gq.get_mut(entity)` arming `GlobalTransform`'s `TRACK_CHANGES` dirty
set for every billboard entity on a frame where nothing moved. The file's own header comment says so:

> *"Without this gate, `world_bound_propagation`'s incremental-bounds fast path was defeated every frame
> in billboard-heavy cells (vegetation impostors, sprite quads)."*

Before this delta the gate was unconditional:

```rust
// 73896726~1, byroredux/src/systems/billboard.rs:56
if last_cam == Some((cam_pos, cam_forward)) { return; }
```

It is now:

```rust
// HEAD
if last_cam == Some((cam_pos, cam_forward)) && !wind_active && !wind_state_changed { return; }
```

with `wind_active = wind.speed > 1.0e-4 || wind.gust_amplitude > 1.0e-4`.

`WindField` is installed for **every exterior worldspace** (`byroredux/src/scene/world_setup.rs`,
`install_ground_cover`) from `WindField::from_weather_byte(WTHR.wind_speed, …)`, so **any weather with a
non-zero wind byte makes `wind_active` permanently true**. The early-out then never fires outdoors, and
the loop runs at full cost every frame with the camera stationary.

### And the loop is not scoped to trees

The `Billboard` walk iterates **all** `Billboard` entities and unconditionally does `gq.get_mut(entity)`
followed by `global.rotation = new_rot`, applying the wind only afterwards and only when
`tree_wind.is_some()`. So an FX impostor or sprite quad with **no `SpeedTreeWind`** is still re-fetched
mutably and re-written with a **bit-identical** rotation every frame purely because the weather is windy.
The write is redundant; **the dirty-set arming is not.**

`wind_state_changed = last_wind != Some(wind)` is the correct fix for the
stationary-camera-weather-transition case it was added for (`0304538c`) — the problem is that
`wind_active` in the same condition makes it moot: the gate is already open.

## Evidence

- The file header comment stating the gate's purpose.
- `git show 73896726~1:byroredux/src/systems/billboard.rs` line 56 (`if last_cam == Some((cam_pos, cam_forward)) { return; }`)
  vs HEAD's three-term condition — the exact widening.
- `git log --reverse -S "wind_active" -- byroredux/src/systems/billboard.rs` → **`73896726`**, i.e.
  inside this delta window.
- `byroredux/src/scene/world_setup.rs` — `WindField` installed unconditionally for exteriors;
  `groundcover_translate.rs` — speed comes from the WTHR byte, so it is non-zero for most vanilla
  exterior weathers.
- `byroredux/src/systems/billboard.rs` — `gq.get_mut(entity)` and `global.rotation = new_rot` are
  **outside** the `tree_wind.is_some()` branch.

## Impact

In exactly the cells SpeedTree exists for — tree-heavy vanilla exteriors, hundreds of billboard entities
— the incremental world-bound propagation fast path is **defeated every frame whenever the weather is
windy, including with the camera parked**. This is the cost #1374 was closed to remove, now re-incurred
by default and extended to non-SpeedTree billboards that gain nothing from it.

CPU-side only; no correctness impact.

## Suggested Fix

Split the two motivations:

- When the camera has **not** moved, skip the full `Billboard` walk and update only the entities that
  actually need a wind refresh — iterate `SpeedTreeWind` (already queried in the same function) and
  intersect with `Billboard`, rather than the reverse.
- Keep the unconditional full walk for the camera-moved case.

Regression guard: park the camera under active wind with one `Billboard`-without-`SpeedTreeWind` entity
and assert its `GlobalTransform` is **not** re-written.

## Related

- **#1374** (CLOSED) — the issue whose guarantee this weakens. Its guard did not cover the wind term.
- **#823**, **#829** — the sibling billboard hot-path fixes.
- **#3137** (`PERF-D1-02`, OPEN) — notes in passing that the billboard `retain` is "not gated in
  practice" for the same reason, but scopes its finding to hashing/allocation; the gate defect itself is
  not covered there.

## Completeness Checks

- [ ] **SIBLING**: the geometry arm of the same system has the same unconditional `get_mut` shape — check
      it under the new gating
- [ ] **LOCK_ORDER**: the single `query_mut::<GlobalTransform>()` handle pattern (#829) is preserved — do
      not reintroduce a read lock + write lock pair
- [ ] **TESTS**: a guard pinning that a parked camera under active wind does not re-dirty a
      `Billboard`-without-`SpeedTreeWind` entity — i.e. #1374's guarantee is re-pinned including the wind
      term
