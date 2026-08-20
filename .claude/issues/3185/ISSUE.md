# NIFAL-D1-2026-08-20-03: mesh-water classifiers introduce four uncited constants, including a hardcoded world +X current direction that feeds physics drag on every river mesh

Issue: https://github.com/matiaszanolli/ByroRedux/issues/3185
Finding: NIFAL-D1-2026-08-20-03
Labels: medium,nif-parser,renderer,bug
Source: docs/audits/AUDIT_NIFAL_2026-08-20.md

Filed from `docs/audits/AUDIT_NIFAL_2026-08-20.md` (Dimension 1 — Material, mesh-water slice). NIFAL canonical-translation finding — see `/audit-nifal`.

**Severity**: MEDIUM
**Tier violated**: `no-fabrication`
**Game Affected**: all — the name heuristic and geometry fallback are game-agnostic and run on every water-shader mesh.

**Location**: `byroredux/src/material_translate.rs:173`, `:211`, `:236`

## The headline: a river flowing north pushes the player east

Of the three mesh-water findings in this sweep this is **the most actionable**, because constant (1) below is not a cosmetic default — it reaches **physics current drag**, not just UV scroll. `WaterFlow.direction` is handed a hardcoded world **+X** for *every* river / stream / canal mesh **regardless of that mesh's placement rotation**. A river mesh placed to run north–south gets a current pushing perpendicular to its own channel, shoving the player sideways out of the water. The block already holds the placement rotation in hand — it passes it to `water_volume_from_mesh` on the very next statement.

## Read this together with its two siblings

One of **three** mesh-water findings. Mesh water crossed NIFAL for the first time in this delta with the derivations centralised but not the boundary discipline — the duplication `translate_material` exists to prevent, re-created for a new category. The siblings are the 39-line verbatim spawn-block copy-paste and the `WaterKind -> foam_strength` divergence. Same fix pass.

## Description

The NIFAL layer's `no-fabrication` invariant requires a new constant to cite a measurement or source (see the `feedback_no_guessing` project rule). The mesh-water slice added **four** that do not:

1. **`[1.0, 0.0, 0.0]` — the river/rapids flow direction** (`:173`). `WaterFlow.direction` is documented on the canonical type as *"Unit vector in **world Y-up space**"* (`crates/core/src/ecs/components/water.rs:396`), and the EXAL sibling derives it from real authored data — WATR `NAM0` linear velocity when present, otherwise the `wind_direction` angle after the Z->Y swizzle (`byroredux/src/env_translate.rs:962-980`). The NIFAL path hands it a constant world +X unconditionally.
2. **`spans[1] > 16.0`** (`:211`) — the vertical-extent floor for the waterfall geometry fallback. Units are un-stated (post-import Y-up game units, so 16 is roughly 0.23 m at Skyrim's ~70 units/m) and no corpus measurement is cited.
3. **`spans[1] > horizontal * 1.5`** (`:211`) — the tall-and-narrow aspect ratio. Reasoned in prose ("horizontal rivers/lakes have their largest span in X/Z") but not measured.
4. **`radius * 4.0`** (`:236`) — the synthesized underwater depth of a mesh water volume, which sets `WaterVolume.min.y` and therefore how deep `submersion_system` believes an actor can sink.

This is precisely the shape the layer's own reference cases are contrasted against: the emissive scale is a *measured* no-op and the particle `initial_color` is a *deliberate* non-application, both with the evidence written down at the site.

## Evidence

```rust
// byroredux/src/material_translate.rs:169-175
let flow = match kind {
    WaterKind::Calm => None,
    WaterKind::Waterfall => Some(WaterFlow::for_kind(kind, [0.0, -1.0, 0.0])),
    WaterKind::River | WaterKind::Rapids => {
        Some(WaterFlow::for_kind(kind, [1.0, 0.0, 0.0]))   // <- world +X, always
    }
};

// byroredux/src/material_translate.rs:209-215
let horizontal = spans[0].max(spans[2]).max(1.0);
if spans[1] > 16.0 && spans[1] > horizontal * 1.5 {   // <- both uncited

// byroredux/src/material_translate.rs:233-239
min: [center.x - radius, position.y - radius * 4.0, center.z - radius],  // <- uncited
```

The `[0.0, -1.0, 0.0]` waterfall direction is **not** part of this finding — "falls are downward in Y-up" is stated as the canonical convention on the component doc itself and is self-evidently sourced.

## Impact

The flow direction reaches **both** the shader UV scroll bias **and** `crates/physics/src/water.rs`'s current drag on dynamic bodies and actor swim resistance — so a mis-oriented river pushes the player sideways out of the channel. That is a gameplay effect, not merely a visual one.

The geometry thresholds decide River-vs-Waterfall, which in turn decides whether the mesh gets a swimmable `WaterVolume` at all (`nif_loader.rs:1055` / `mesh_instance.rs:759` skip the volume for `Waterfall`), so a mis-fire silently removes swimmability from a body of water.

## Suggested Fix

For (1): derive the horizontal direction from the mesh's own longest horizontal principal axis (the positions array is already in hand) **rotated by the placement quaternion** — or emit no `WaterFlow` at all rather than a fabricated one. Either is sourced; the current value is not.

For (2)–(4): either cite a corpus measurement over the installed Oblivion/FNV/Skyrim water meshes at the constant, or name them (`WATERFALL_MIN_VERTICAL_SPAN`, `WATERFALL_ASPECT_RATIO`, `MESH_WATER_DEPTH_RADII`) with an explicit "unmeasured placeholder" note so the next sweep can tell a measurement from a guess.

## Related

- Sibling findings from the same sweep (the duplicated spawn block; the `foam_strength` divergence).
- `feedback_no_guessing` (project rule: never guess values/heuristics — research docs, source, papers first).
- #2872 — the WATR `wind_speed` constant-90.0 investigation, the precedent for "a value with no variance is not an authored source, say so at the site".

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: the derivations stay at the NIFAL boundary in `byroredux/src/material_translate.rs` — no per-game or per-placement logic pushed into the shader or re-derived at render time. See `/audit-nifal`.
- [ ] **PHYSICS**: the flow-direction fix is verified against `crates/physics/src/water.rs` current drag, not only against the shader's UV scroll
- [ ] **SIBLING**: the EXAL producer (`byroredux/src/env_translate.rs:962-980`) and the NIFAL producer end up with a comparable sourcing story
- [ ] **NO-FABRICATION**: every surviving constant carries a measurement, a source, or an explicit "unmeasured placeholder" note at the site
- [ ] **TESTS**: a rotated river mesh asserts a flow direction aligned with its own channel, not world +X
