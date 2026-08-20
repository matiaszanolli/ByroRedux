# #3191 — SPT-D2-2026-08-20-01: the wind bend is composed in the object-local frame but weighted by world-space wind components

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3191
- **Labels**: `medium,renderer,bug`
- **Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: MEDIUM
**Dimension**: Placeholder Fallback (secondary: TREE→Billboard Wiring)
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-20.md` (`SPT-D2-2026-08-20-01`) — HEAD `bb0b92f2`

## Location

- `byroredux/src/systems/billboard.rs` — `apply_speedtree_wind` (the returned composition and the
  `along_weight` / `cross_weight` derivation), called from the billboard arm and the geometry arm of
  `make_billboard_system`
- `byroredux/src/systems/billboard.rs` — `compute_billboard_rotation`
  (`Quat::from_rotation_arc(-Vec3::Z, look_dir)`)
- `crates/spt/src/import/mod.rs` — placeholder quad layout / pivot

## Status

NEW — introduced by `4ddf7062`, weighting added by `6f67c79b`.

## Description

`apply_speedtree_wind` returns

```rust
base * Quat::from_rotation_z(wave * bend * along_weight)
     * Quat::from_rotation_x(cross * bend * 0.65 * cross_weight)
```

Post-multiplication means both rotations are applied in the **object frame**, before `base`. The weights
that select between them are **world-space**: `along_weight = |wind_dir.x|` (world X) and
`cross_weight = |wind_dir.y|` (world Z).

That pairing is coherent for the **geometry** consumer, where `base` is the authored near-identity world
rotation: local Z ≈ world Z, so `Rz` leans the trunk along ±X and `Rx` leans it along ±Z, each weighted
by the matching wind component.

It is **not** coherent for the **billboard** consumer, where
`base = Quat::from_rotation_arc(-Vec3::Z, look_dir)` maps object `-Z` onto the direction of the camera.
Object-local Z is therefore the **view axis**, and:

- **`Rz` becomes a roll in the screen plane**, pivoting at the quad's origin (the trunk base — the quad
  spans `y ∈ [0, height]`, `x ∈ ±w/2`, `z = 0`). It reads as a lean, but a lean whose on-screen
  direction is fixed by the camera, not by the wind. Orbit the tree and the lean stays put; stand
  downwind, where a real tree leans away from you and shows almost no lateral motion, and it still sways
  left-right.
- **`Rx` becomes a pitch about the screen-horizontal axis**, tipping the flat card toward or away from
  the viewer. A zero-thickness card tipped about its base does not lean — it **foreshortens**, so the
  tree's apparent height pulses with the gust.

### The wind's sign never reaches the bend

Independently of the frame issue, both weights are `.abs()`, and `wave` is a zero-mean sine, so there is
**no mean lean at all**. Reversing the wind direction changes only `phase` (a travelling-wave offset),
never which way the tree leans. Real wind produces a sustained lean plus oscillation about it; this
produces oscillation about vertical whose amplitude depends only on `|wind_dir|` components, with a
`.max(0.25)` floor that keeps a cross-bend alive even when the wind is exactly axis-aligned.

## Evidence

- `apply_speedtree_wind`'s `.abs()` weights and the post-multiplied local-frame rotations, confirmed at
  HEAD.
- `compute_billboard_rotation`'s `Quat::from_rotation_arc(from, look_dir)` with `from = -Vec3::Z`,
  establishing object `-Z` = toward camera, hence object `Z` = view axis.
- `crates/spt/src/import/mod.rs` — quad vertices with origin at bottom-centre, which is what makes `Rz`
  read as a base-pivoted lean and `Rx` read as foreshortening.
- `wave` / `cross` are zero-mean `sin` / `cos`; no mean term exists anywhere in the expression.
- **The existing test cannot catch this**: it asserts only that direction `[1,0]` and `[0,1]` give
  *different* results, which the differing `along_weight` / `cross_weight` satisfy. It cannot distinguish
  a correct directional lean from an amplitude-only reweighting.

## Impact

Visual fidelity on every `.spt` tree in three games whenever weather wind is non-calm. The sway is
view-locked (wrong as the camera orbits) and gust-synchronised vertical pulsing is visible on the card.

The wider point: the shared wind model is tuned for the **geometry** consumer and applied unchanged to
the **billboard** consumer — which is the only one that is actually reachable today (see the sibling
finding on the dead geometry branch).

## Suggested Fix

Build the bend in **world space** and conjugate it into the entity's frame, rather than post-multiplying
object-local axis rotations — i.e. form the lean as a rotation about the world-horizontal axis
perpendicular to `wind_dir` (`axis = (−wind_dir.y, 0, wind_dir.x)`) and **pre**-multiply:
`Quat::from_axis_angle(axis, angle) * base`.

Carry the wind's sign by giving the lean a non-zero mean (`mean + osc·sin(...)`) instead of weighting a
zero-mean sine with `.abs()` magnitudes.

Guards:
- a test that orbits the camera 180° around a fixed tree under fixed wind and asserts the **world-space**
  lean direction is unchanged;
- a test asserting `dir` and `−dir` produce **opposite** mean leans.

## Related

- The sibling finding that the geometry consumer this model *was* written for is unreachable.
- **#1000** — the `-Z` front-face convention this interacts with.
- **#1715**.
- **#3190** (`SPT-D3-2026-08-20-01`) — the *input* half of the same handoff; this is the *output* half.

## Completeness Checks

- [ ] **SIBLING**: both `apply_speedtree_wind` call sites (billboard arm + geometry arm) remain correct
      under the new composition — the geometry arm is the one the current model was right for
- [ ] **TESTS**: camera-orbit invariance and wind-sign reversal are both pinned; the existing
      "different directions give different results" assertion is not sufficient
