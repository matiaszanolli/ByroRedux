# #3526 — SF-2026-08-27b-D2-01: canonical_mesh_path's "vanilla is unaffected" premise is false — 13,713 vanilla FaceGen names are already fully composed

Source: `docs/audits/AUDIT_STARFIELD_2026-08-27b.md`
Filed: 2026-08-28 (`/audit-publish`)
Labels: low, documentation, doc-rot, nif-parser, nif, game:starfield, legacy-compat

---

From `docs/audits/AUDIT_STARFIELD_2026-08-27b.md` (branch `main` @ `969d81c8`).

- **Severity**: LOW
- **Dimension**: 2 — BSGeometry mesh extraction (documentation / premise)
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs` — the `canonical_mesh_path` doc comment; corroborating block-level doc at `crates/nif/src/blocks/bs_geometry.rs`
- **Type**: doc/premise only — the code is correct

## Description

`canonical_mesh_path`'s doc comment justifies #2361's head/tail-detection fix with:

```rust
/// A name already carrying `geometries\` and/or `.mesh` composed into
/// `geometries\geometries\x.mesh.mesh` — a guaranteed miss, and a silent one
/// until #2357 gave the resolve-miss path a log. Vanilla is unaffected (every
/// sampled `.mesh` name is a bare 20-hex stem); this is authoring-tool output
/// and mods that use readable paths.
```

The parenthetical is false against retail data. A full scan of every `BSGeometry` external mesh name in all five vanilla mesh archives finds **13,713 already fully composed names**, all of them in `Starfield - FaceMeshes.ba2` — that is **100% of that archive's external mesh names**, and the `.mesh` files really do live at exactly the composed path in the same archive.

## Evidence

| Archive | external names | non-ASCII | already headed | already tailed | name length |
|---|---|---|---|---|---|
| Meshes01 | 430 418 | 0 | 0 | 0 | 41 (uniform) |
| Meshes02 | 0 | — | — | — | — |
| MeshesPatch | 388 982 | 0 | 0 | 0 | 41 (uniform) |
| LODMeshes | 48 959 | 0 | 0 | 0 | 41 (uniform) |
| **FaceMeshes** | **13 713** | 0 | **13 713** | **13 713** | **57 (uniform)** |

Sample, from `meshes\actors\character\facegendata\facegeom\starfield.esm\0026fdf0.nif`:
`"Geometries\\526277e35270101cf88e\\9b0d60d3a60db8befad9.mesh"` — and `geometries\526277e35270101cf88e\9b0d60d3a60db8befad9.mesh` is present in `Starfield - FaceMeshes.ba2` (6 114 entries, 4 832 of them under `geometries\`). Lookup case is not an issue: `Ba2Archive::extract` normalises through `normalize_path` (`crates/bsa/src/ba2.rs`), which lowercases, so the capitalised `Geometries\` head resolves.

## Impact

No runtime defect — the current code takes the `(true, true)` arm and passes the name through verbatim, which is right. The cost is epistemic and pointed: pre-#2361 this composition produced `geometries\Geometries\<hash>\<hash>.mesh.mesh` for **every vanilla Starfield FaceGen head-geometry reference**, so #2361 was a vanilla-content fix, not the mods-only hardening its own comment describes. A future reader deciding how carefully to guard that helper is reading an understatement.

The same parenthetical is also what the 2026-08-27 report used to bound #3391's blast radius — the *conclusion* there survives (0 non-ASCII names in 882,072 vanilla external mesh names, confirmed by the same scan), but its stated premise does not.

## Related

#2361 (`61520a39`), #1292, #2357, #3391 (CLOSED), and #3464 (`BSFaceGenNiNode` 2-byte under-read on the same 1,417 facegen head nodes, filed concurrently by the NIF pass).

## Suggested Fix

Replace the parenthetical with the measurement — the vanilla corpus is 868,359 bare 41-char stems (Meshes01/02/Patch/LODMeshes) **plus 13,713 fully-composed 57-char paths in `FaceMeshes.ba2`**, so both arms of the head/tail test are exercised by vanilla content. Consider adding a seventh unit test using a real FaceMeshes-shaped name.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the block-level doc in `crates/nif/src/blocks/bs_geometry.rs` carries the corroborating claim)
- [ ] **TESTS**: A regression test pins this specific fix
