# #3777: SF-D2-01: every Starfield facegen .mesh body fails to parse — 1,282/1,282 FaceMeshes NIFs import ZERO geometry, every NPC renders headless

**Labels**: bug, nif-parser, import-pipeline, high, legacy-compat, nif, game:starfield
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_STARFIELD_2026-08-30.md` — SF-D2-01 (HIGH)
**Dimension**: 2 — BSGeometry `.mesh` extraction
**Location**: `crates/nif/src/blocks/bs_geometry.rs` (`BSGeometryMeshData::parse`, the `n_meshlets` / `n_cull_data` reads immediately after the LOD array), cascading through `crates/nif/src/import/mesh/bs_geometry.rs` (Stage B LOD-slot loop)

## Description

`BSGeometryMeshData::parse` reads a meshlet + cull-data trailer **unconditionally** after the LOD array:

```rust
let n_lods = stream.read_u32_le()?;
for _ in 0..n_lods { … }
let n_meshlets = stream.read_u32_le()?;          // <-- EOF here on facegen bodies
let meshlets = stream.read_pod_vec::<Meshlet>(n_meshlets as usize)?;
let n_cull_data = stream.read_u32_le()?;
let cull_data = stream.read_pod_vec::<CullData>(n_cull_data as usize)?;
```

Starfield facegen `.mesh` bodies ship **no such trailer** — they end exactly at the last LOD entry. Every one of them therefore fails with `failed to fill whole buffer`, and every FaceMeshes NIF ends up with zero geometry.

Verified at HEAD (`64f64480`) by symbol, not line number: the four reads above are still unconditional and there is no EOF-tolerant arm.

## Evidence

Field-by-field walk classifying where each of the **680,239** vanilla `.mesh` bodies ends:

```
GLOBAL: {"ENDS_AFTER_LODS": 4832, "FULL_EXACT": 675407}
of which carry skin weights: {"ENDS_AFTER_LODS": 4832, "FULL_EXACT": 10185}
```

`FULL_EXACT` = byte-exact fit through `cull_data`. The split is **100% clean along the FaceMeshes archive boundary** — all 4,832 facegen bodies, none of the other 675,407. It is **not** "skinned bodies omit the trailer": 10,185 `FULL_EXACT` bodies also carry skin weights.

Worked example (`geometries\04849f0a968b16012bb3\88ae67fe92bc26895682.mesh`, 38,940 B) — every preceding field consumes exactly the right bytes:

```
version=2 @4 | n_tri_indices=4200 @8 | after_tris @8408 | scale=1.0 @8412
weights_per_vert=1 @8416 | n_vertices=1386 → @16736 | n_uv0=1386 → @22284
n_uv1=0 | n_colors=0 | n_normals=1386 → @27840 | n_tangents=1386 → @33388
n_total_weights=1386 → @38936 | n_lods=0 → cursor 38940 == file_len 38940
                                          ^ parser reads n_meshlets here → EOF
```

**Cascade**: `parse_from_bytes` → `Err("failed to fill whole buffer")` → Stage B's `Err(e)` arm logs at **`debug!`** and `continue`s → all LOD slots exhaust → `log::warn!` → `return None` → the shape never becomes an `ImportedMesh`.

**End-to-end** (real `Ba2Archive`-backed `MeshResolver`, `parse_nif` + `import_nif_scene_with_resolver`, every NIF in each archive):

```
Starfield - FaceMeshes.ba2
  nifs=1282 imported=1282 nif_parse_fail=0 with_mesh=0 ZERO_MESH=1282 total_meshes=0
Starfield - Meshes01.ba2
  nifs=31058 imported=31058 nif_parse_fail=0 with_mesh=29213 ZERO_MESH=1845 total_meshes=172907
```

Re-confirmed under a realistic 4-archive resolver: FaceMeshes stays at 1,282/1,282 zero. Not a resolver-scope artifact.

## Impact

**Every Starfield NPC renders headless.** 1,282 / 1,282 FaceMeshes NIFs import zero meshes; 4,832 / 4,832 facegen `.mesh` bodies fail to parse. 100% loss of a content class.

**Why no existing gate catches it**: `nif_parse_fail = 0` — the `.nif` *files* parse perfectly. `crates/nif/tests/parse_real_nifs.rs` gates FaceMeshes at `min_clean: 0.995` and it measures **100.00%**; `ROADMAP.md` records FaceMeshes at 100.00%. That gate reads `.nif` blocks; the external `.mesh` companion bodies it never opens are where the failure lives. The compat matrix is simultaneously correct and blind to a total content loss.

Severity: `_audit-severity.md` puts "NIF parse failures that prevent loading game content" at HIGH, and its special-rules table sets "NIF parse failure (hard error)" at HIGH minimum. Loss is 100% of a content class but recoverable and non-corrupting → HIGH, not CRITICAL.

## Suggested Fix

Bounded, and it requires **no invented field semantics**:

1. Treat EOF at the `n_meshlets` read as "no trailer present" — empty `meshlets` / `cull_data`, exactly as the `scale <= 0` sentinel arm already returns an all-empty body.
2. Gate it on the cursor being **exactly** at EOF, so a genuinely truncated body still errors. Everything before the trailer already decodes with an exact byte fit; no other field changes.
3. Add a FaceMeshes-shaped fixture.
4. Extend the parse-rate gate to open `.mesh` companions — the current gate structurally cannot see this class of defect.

## Related

- #3526 (FaceMeshes *path* composition) and #3464 (`BSFaceGenNiNode` 2-byte under-read) touch the same archive but are different defects; neither would fix this.
- #3549 (Starfield skinned bones unresolved) is downstream of a different mechanism.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related parsers (`BSGeometry` internal-mesh path, other trailing-array reads in `bs_geometry.rs`)
- [ ] **CANONICAL-BOUNDARY**: The fix stays in the parser; no per-game logic reaches the importer or the renderer. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix — a synthetic FaceMeshes-shaped body that ends exactly after the LOD array must parse to `meshlets.is_empty() && cull_data.is_empty()`, and a body truncated *mid*-LOD must still error
- [ ] **GATE**: `crates/nif/tests/parse_real_nifs.rs` extended to open `.mesh` companions so this class of defect is visible to CI
