# #2193 investigation

## Prior state (commit `a4c11bfb`, 2026-07-27)

The previous investigation de-duplicated the NiTriStrips de-stripper
(`resolve_tri_strips_data_refs` now calls the canonical
`NiTriStripsData::to_triangles`, shared with the render path) and added two
regression tests that **ruled out** two hypotheses:

- The strip-parity (odd/even triangle) de-stripping rule — proven
  equivalent to the canonical one (cyclic rotation, same orientation).
- The Z-up→Y-up coordinate swap (`havok_to_engine`) — proven to be a proper
  rotation (det = +1) that cannot flip triangle orientation.

Root cause was left open: isolating it further was said to need a live
Vulkan device + real Oblivion game data (`ICMarketDistrictTheGildedCarafe`),
neither available to that investigation or this one.

## New finding this session

`bhkPackedNiTriStripsShape` / `hkPackedNiTriStripsData` — a *different*,
previously-uninvestigated collision path, very commonly used for Oblivion
static architecture (`resolve_packed_mesh` in `shape.rs`) — was never
checked. `nif.xml:2246-2251` documents `TriangleData`:

```
<field name="Triangle" type="Triangle">The triangle.</field>
<field name="Welding Info" type="bhkWeldInfo">...</field>
<field name="Normal" type="Vector3" until="20.0.0.5">This is the triangle's normal.</field>
```

Oblivion is NIF version 20.0.0.5 exactly, so every packed collision
triangle in Oblivion content carries an **authored face normal**,
independent of the `[v0, v1, v2]` index winding. The parser already reads
this into `PackedTriangle::normal` (`shape_mesh.rs:141-158`), but
`resolve_packed_mesh` never read it back — the collision normal was always
whatever the raw winding order happened to derive (`cross(v1-v0, v2-v0)`,
the same convention Parry3D's `TriMesh` uses for contact resolution).

This is a plausible, non-blanket explanation for a *localized* inverted
normal: if the winding and the separately-authored normal usually agree
(explaining why most Oblivion floors ground correctly today) but disagree
for specific triangles in specific content, only those triangles would
produce inverted contacts — matching the reported symptom's profile
exactly (one specific interior, not every Oblivion floor).

FO3+ has no equivalent field (nif.xml folds a differently-encoded normal
into `Welding Info` instead — `shape_mesh.rs:149`), so this is naturally
scoped to Oblivion only and cannot regress the FO3/FNV/Skyrim SE path,
which the issue's SIBLING check asked to confirm.

## Fix applied

`packed_triangle_winding()` (`shape.rs`): when `t.normal` is `Some`,
compute the raw geometric normal from the stored winding and compare it
against the authored one. On disagreement (`dot < 0.0`), swap `v1`/`v2` so
the imported triangle's derived normal matches Bethesda's authored intent.
Two regression tests pin both directions: a disagreeing triangle gets
corrected (and reads +Y/up in engine space, the `is_grounded` observable),
and an already-agreeing triangle is left untouched.

## What's still unconfirmed

This is a strong, spec-backed candidate — a previously-unused authored
field that maps directly onto the reported symptom's shape — but it has
**not** been confirmed live against `ICMarketDistrictTheGildedCarafe`
(requires a Vulkan device + real Oblivion game data, unavailable in this
environment, same limitation the prior investigation hit). If `is_grounded`
still reads false there after this fix, the remaining candidates are: (a)
the specific floor uses a different shape path entirely (compound/list
wrapping, MOPP tree pruning a triangle at that exact spot), or (b) the
grounding threshold/contact-normal consumption in `crates/physics` itself.
