Surfaced 2026-05-28 in the first Cydonia render attempt (Phase 5 of [docs/engine/starfield-esm-roadmap.md](docs/engine/starfield-esm-roadmap.md)). **Top blocker for visible Starfield rendering.**

## Symptom

Loading `CityCydoniaMainLevel` (form 0x002B3DA2, **27 898 placed REFRs**) produces **75 entities** in the scene. **99.73% silent-drop.** byro-dbg `stats`:

```
Entities: 32 998 | Meshes: 3/0 | Textures: 2/0 | Draws: 1 cmds → 0 batches → 0 GPU calls
```

(32 998 = 75 cell + player rig + camera + debug; 3 unique mesh handles registered; 0 in active use → rendering nothing.)

The drop fires this warn 1 624 times — one per distinct silently-broken NIF:

```
WARN  byroredux::cell_loader::references
NIF 'meshes\SetDressing\Posters\FFCydoniaZ04_SpaceFrog_Poster_01.nif' imported with
zero meshes / collisions / lights / emitters / clips — likely CSG-deferred
(`_oc.nif` Shared variant, #1188) or pure marker scene
```

The diagnostic is misleading — these aren't CSG-deferred (that's FO4). They're real NIFs the parser accepts without error but the importer can't extract geometry from.

## Affected categories (1 624 distinct paths)

| Category | Count | Examples |
|----------|------:|----------|
| SetDressing | 1 040 + 28 case-variant | Posters / computers / fans / toolboxes / bathrooms / wet-floor signs / whiteboards |
| Architecture | 403 + 5 case-variant | GenKit interiors, IndustrialKit ext/int, OPM kit |
| Landscape | 56 | (terrain LOD chunks?) |
| Items | 31 | |
| Furniture | 17 | |
| Ships | 13 | |
| StarStations | 13 | |
| Markers | 11 | (subset legitimately marker scenes — expected drop) |

The Posters case is the smoking gun. Cydonia posters are flat textured planes — should be the simplest possible content. Returning zero meshes isn't CSG/marker; it's the import path failing to find the geometry block.

## Likely root cause

Starfield's `BSGeometry` block stores per-vertex data in **external `.mesh` companion files** (FO76+ pattern). The 2026-05-28 Starfield audit ([docs/audits/AUDIT_STARFIELD_2026-05-28.md](docs/audits/AUDIT_STARFIELD_2026-05-28.md) Dim 4) reported "fully wired — inline + external via MeshResolver" but runtime evidence contradicts:

- Cydonia REFRs reference 27 898 base meshes
- 1 624 distinct NIFs come back with zero geometry
- NIF parser HEALTHY — only 147 minor warnings (BSEffectShaderProperty / BSLightingShaderProperty block-size drift), all recovered

So either:
1. **MeshResolver isn't being invoked** for these NIFs (wrong BSVER gate, wrong BSGeometry variant detection).
2. **MeshResolver IS invoked** but the `.mesh` companions aren't in `Starfield - Meshes01.ba2` (maybe `LODMeshes` or per-region archive).
3. **Companions ARE found** but geometry isn't plumbed into `ImportedMesh.{positions, indices}` — the consumer-side gap that mirrors the now-closed [#1289](https://github.com/matiaszanolli/ByroRedux/issues/1289) CDB consumer pattern.

## Suggested investigation path

1. Pick a representative dropped NIF (`meshes\SetDressing\Posters\FFCydoniaZ04_SpaceFrog_Poster_01.nif`).
2. Run `crates/nif/examples/nif_stats` against it to dump the block list — confirm BSGeometry presence + check inline-vs-companion-ref.
3. If companion ref: confirm extraction from `Starfield - Meshes01.ba2` via `crates/bsa/examples/dump_ba2_index`. If absent, locate which archive ships it.
4. If extraction works: trace `crates/nif/src/import/mesh/bs_geometry.rs::BSGeometryMeshKind::External` to confirm consumer-side plumbing into `ImportedMesh`. This is the parser-landed-consumer-unwired pattern (#1289 sibling).
5. Once root cause is clear, file follow-up sub-issues per fix tier.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: FO76 audit — same BSGeometry external-mesh pattern; verify FO76 rendering tests aren't masking the same gap
- [ ] **TESTS**: Once root cause identified, add regression that loads a known-broken Cydonia NIF + asserts non-zero geometry

## References

- Cydonia log: `/tmp/audit/runtime/cydonia.engine.log` (15 029 lines)
- Prior audit: [docs/audits/AUDIT_STARFIELD_2026-05-28.md](docs/audits/AUDIT_STARFIELD_2026-05-28.md) Dim 4
- Sibling pattern: [#1289](https://github.com/matiaszanolli/ByroRedux/issues/1289)
- BSGeometry import: [crates/nif/src/import/mesh/bs_geometry.rs](crates/nif/src/import/mesh/bs_geometry.rs)
