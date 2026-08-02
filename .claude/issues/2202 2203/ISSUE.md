# Issues 2202 + 2203

## #2202: SKY-2026-07-26-01 — BleakFallsBarrow01 entrance vestibule has no floor collider — player free-falls from door spawn
- Severity: HIGH
- Labels: bug, import-pipeline, high, legacy-compat
- State: OPEN
- Location: mechanism not isolated -- candidates at crates/nif/src/import/collision/mod.rs:303 (extract_from_classic reclassification gate), crates/nif/src/import/collision/shape.rs:78 (resolve_shape_inner -> None arms), byroredux/src/cell_loader/spawn.rs:1381 (synthesized-trimesh fallback gate). Symptom consumed at byroredux/src/scene.rs:648-681 (door-spawn floor ladder) and byroredux/src/systems/character.rs:195,219.
- Summary: BleakFallsBarrow01 entrance vestibule has NO fixed collider beneath the door spawn while the cell has 2560 fixed colliders elsewhere -> player capsule free-falls, black screen, all assets loaded.
- 4 candidate mechanisms (A/B/C/D), not isolated:
  (A) partial bhk resolve suppresses trimesh fallback (collisions_empty computed per-NIF not per-shape)
  (B) vestibule floor is Dynamic not Fixed (motion_type Dynamic + nonzero mass, invisible to Fixed census + excluded by QueryFilter::exclude_dynamic())
  (C) collider exists as Fixed but mis-transformed (GlobalTransform::compose_trs / havok_scale error)
  (D) vestibule architecture REFRs never spawn (REFR-level gap, not collision bug)
- Suggested first step: land an (A)/(B)/(C)/(D) census as log::debug! on the spawn path BEFORE touching import code (no-guessing policy) -- dump every collider within XZ radius of door grouped by Rapier body type + PhysicsSourceForm.
- Control (must keep working): --cell WhiterunDragonsreach grounds at frame 0 via rung 2.
- Related: #2203 is the strongest root-cause candidate (mechanism A -- partial resolve leaving collisions non-empty while missing the floor chunk).

## #2203: NIFAL-D6-01 — resolve_compressed_mesh divides chunk indices by 3 — 77% of Skyrim's authored collision geometry destroyed
- Severity: HIGH
- Labels: bug, nif-parser, high, nif
- State: OPEN
- Dimension: 6 (Collision), tiers violated: no-fabrication + no-leak
- Games affected: Skyrim LE/SE + all DLC/CC (every game shipping bhkCompressedMeshShape). Oblivion/FO3/FNV unaffected; FO4+ never reaches this arm.
- Location: crates/nif/src/import/collision/shape.rs:498-535 -- divisor at :505-507 (list path) and :527-529 (strip path)
- Root cause: resolve_compressed_mesh divides every chunk index by 3, per a comment claiming indices are component-indexed (pre-multiplied by 3). Measured against vanilla Skyrim - Meshes0.bsa: 100% of chunks have indices that are VERTEX-indexed (max index < n), 0% component-indexed. nif.xml documents "Num Vertices" (not Indices) as the field multiplied by 3, and the parser (compressed_mesh.rs:172-174) already divides that out at parse time -- the importer double-applies a divisor already consumed.
- Impact: 505,248 of 2,199,732 authored collision triangles survive (23%). 4,415/22,535 chunks (19.6%) yield ZERO triangles. When ALL of a NIF's chunks void out, extract_collision still returns Some(TriMesh{indices:[]}) -> collisions_empty=false -> SUPPRESSES synthesize_static_trimesh fallback -> degrades to SharedShape::ball(1e-3) point collider with no fallback. Bleak Falls Barrow tileset: 75% of collision surface missing.
- Related: strongest root-cause candidate for #2202. Same function also has sibling findings NIFAL-D6-04 (dropped strip residual), NIFAL-D2-02 (strip-walk panic), NIFAL-D6-03 (void Some suppresses fallback), NIFAL-D2-01 (duplicated strip-parity copy) -- NOT all in scope here, "fix together" was suggested but only D6-01's specific /3 bug is this issue.
- Suggested fix: drop the /3 at both sites (list path + strip path). Add resolve_compressed_mesh unit test built from rttempleplazafloorr01 chunk shape, plus corpus assertion no vanilla SSE mesh resolves to zero-triangle TriMesh. Verify against nifly's bhkCompressedMeshShapeData reader before landing.
