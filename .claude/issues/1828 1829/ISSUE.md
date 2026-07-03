# #1828: SF2-01: Stage B external .mesh loop short-circuits on first parsed slot, not first slot with geometry (new gap, external branch, cross-refs #1209)

**Severity**: HIGH
**Dimension**: BSGeometry mesh (Starfield audit Dim 2)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:52-82`
**Related**: Cross-references #1209 (closed) — #1209 fixed the *Stage A* internal-geom branch (`shape.meshes.first()` → `.iter().find_map(...)`, selecting the right slot *kind*). This finding is on the separate *Stage B* external `.mesh` branch (lines 52-77), which #1209 never touched, and is about slot *emptiness*, not slot *kind*. Sibling of SF2-02 (Stage A emptiness variant, filed as a separate issue).

## Description

Stage B (`extract_bs_geometry`, external-mesh branch) iterates `shape.meshes` external slots and `break`s on the first slot whose `.mesh` companion parses `Ok(...)` (lines 58-62). But a vanilla-Starfield `.mesh` slot with the `scale <= 0.0` sentinel parses `Ok` with **empty** `vertices` (populated `triangles`, empty everything else) — this is the documented "segment-only / skin-weight-only slot that shares a parent BSGeometry with a populated slot" case (`crates/nif/src/blocks/bs_geometry.rs:388-390`). The loop takes that empty result as `found`, breaks, and the guard at line 80 (`if mesh_data.vertices.is_empty() || mesh_data.triangles.is_empty() { return None }`) drops the entire BSGeometry — even when a later slot in the same list carries full geometry.

`extract_bs_geometry` returning `None` makes the walker push nothing (`crates/nif/src/import/walk/mod.rs`), so the visible mesh silently vanishes.

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
- [x] **SIBLING**: Same pattern checked in the Stage A internal-geom branch (SF2-02) — both branches need the emptiness check
- [x] **TESTS**: A regression test pins this specific fix (sentinel-before-populated slot ordering)

---

# #1829: SF2-02: Stage A internal-geom find_map accepts the first Internal slot even when its body is empty

**Severity**: MEDIUM
**Dimension**: BSGeometry mesh (Starfield audit Dim 2)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:32-35`
**Related**: Sibling of SF2-01 (Stage B / external-mesh emptiness variant, HIGH). Both stem from the same missing emptiness check, on the two different branches of `extract_bs_geometry`. Not a regression of #1209 — #1209 fixed slot-*kind* selection on this same Stage A branch (`first()` → `find_map()`); this finding is about slot-*emptiness*, which #1209 did not address.

## Description

The Stage A `find_map` (internal-geom branch, fixed for slot-kind selection by #1209) returns the first slot whose `kind` is `Internal { mesh_data }` regardless of whether `mesh_data.vertices` / `triangles` is empty. If an inline `Internal` slot is itself a `scale<=0` sentinel (the format permits inline sentinel slots) and a later `Internal` slot is populated, the post-loop guard at `bs_geometry.rs:80-82` drops the mesh. Lower severity than SF2-01 because vanilla Starfield ships external `.mesh` (inline is authoring-tool / port only per `crates/nif/src/blocks/bs_geometry.rs:208-212`), so the trigger is rarer.

## Suggested Fix

Add the emptiness check to the `find_map` closure:
```rust
BSGeometryMeshKind::Internal { mesh_data }
    if !mesh_data.vertices.is_empty() && !mesh_data.triangles.is_empty() =>
    Some(mesh_data.as_ref()),
```
Fold into the SF2-01 test module.

## Completeness Checks
- [x] **SIBLING**: Same pattern checked in the Stage B external-mesh branch (SF2-01) — both branches need the emptiness check
- [x] **TESTS**: A regression test pins this specific fix (inline sentinel-before-populated slot ordering)
