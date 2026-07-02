# SF2-01: Stage B external .mesh loop short-circuits on first parsed slot, not first slot with geometry (new gap, external branch, cross-refs #1209)

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1828
**Labels**: bug, nif-parser, high, legacy-compat

**Severity**: HIGH
**Dimension**: BSGeometry mesh (Starfield audit Dim 2)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:52-82`
**Related**: Cross-references #1209 (closed) — #1209 fixed the *Stage A* internal-geom branch (`shape.meshes.first()` → `.iter().find_map(...)`, selecting the right slot *kind*). This finding is on the separate *Stage B* external `.mesh` branch (lines 52-77), which #1209 never touched, and is about slot *emptiness*, not slot *kind*. Sibling of SF2-02 (Stage A emptiness variant, filed as a separate issue).

## Description

Stage B (`extract_bs_geometry`, external-mesh branch) iterates `shape.meshes` external slots and `break`s on the first slot whose `.mesh` companion parses `Ok(...)` (lines 58-62). But a vanilla-Starfield `.mesh` slot with the `scale <= 0.0` sentinel parses `Ok` with **empty** `vertices` (populated `triangles`, empty everything else) — this is the documented "segment-only / skin-weight-only slot that shares a parent BSGeometry with a populated slot" case (`crates/nif/src/blocks/bs_geometry.rs:388-390`). The loop takes that empty result as `found`, breaks, and the guard at line 80 (`if mesh_data.vertices.is_empty() || mesh_data.triangles.is_empty() { return None }`) drops the entire BSGeometry — even when a later slot in the same list carries full geometry.

`extract_bs_geometry` returning `None` makes the walker push nothing (`crates/nif/src/import/walk/mod.rs`), so the visible mesh silently vanishes.

## Evidence

- Break-on-first-`Ok`: `crates/nif/src/import/mesh/bs_geometry.rs:58-62`
  ```rust
  match BSGeometryMeshData::parse_from_bytes(&bytes) {
      Ok(data) => {
          found = Some(data);
          break;
      }
      ...
  }
  ```
- Sentinel returns empty-but-`Ok` body: `crates/nif/src/blocks/bs_geometry.rs:391-408` (`if scale <= 0.0 { return Ok(Self { … vertices: Vec::new(), triangles … }) }`); doc comment at `blocks/bs_geometry.rs:388-390` confirms vanilla Starfield uses multi-slot BSGeometry with sentinel slots.
- Post-loop drop: `crates/nif/src/import/mesh/bs_geometry.rs:80-82`.

## Impact

Any Starfield BSGeometry whose slot order places a `scale<=0` sentinel external `.mesh` before the populated one renders as nothing. This is the failure mode #1209 killed on the internal-geom (Stage A) branch, now confirmed live on the external-mesh (Stage B) branch — the ~99% Starfield case per `crates/nif/src/blocks/bs_geometry.rs:9-11`. Blast radius depends on how often vanilla LOD-slot ordering puts a sentinel first; the #1209 precedent shows sentinel-first ordering does occur in practice.

## Suggested Fix

In the Stage B loop, don't `break` on the mere `Ok`; keep iterating past empty results:
```rust
if !data.vertices.is_empty() && !data.triangles.is_empty() {
    found = Some(data);
    break;
}
```
(still `log::debug!` skipped empties). Add a regression test with `meshes = [External(sentinel scale<=0), External(populated)]` asserting a mesh is returned.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the Stage A internal-geom branch (SF2-02) — both branches need the emptiness check
- [ ] **TESTS**: A regression test pins this specific fix (sentinel-before-populated slot ordering)

