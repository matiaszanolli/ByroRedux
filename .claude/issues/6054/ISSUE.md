# FNV-2026-10-18-05: The default radius-12 door exit to WastelandNV saturates the 4 M-vertex geometry pool; 352 distant-LOD uploads are rejected

- **Issue**: #6054 (https://github.com/matiaszanolli/ByroRedux/issues/6054)
- **Labels**: medium, terrain-exterior, renderer, bug, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Origin**: D1-06. **Owner**: `/audit-exterior` / `/audit-renderer`.
- **Location**: `byroredux/src/scene.rs:40` (`DEFAULT_EXTERIOR_RADIUS = 12`; door transitions use it via `app_step.rs::exterior_transition_radius`); `crates/renderer/src/mesh.rs:30-35` (`VERTEX_POOL_HARD_CAP = 4_000_000`, strict admission).
- **Status**: NEW for FNV. `docs/engine/exterior-grid-streaming.md:133` documents the capacity behaviour only in general terms.
- **Evidence**: P0 route at the default radius: 625 full-detail cells, 106,749 entities, 3,759 meshes. Then `Object-LOD '…level4.x-8.y28.nif' mesh upload failed: vertex pool hard cap exceeded: would grow from 3984783 to 4001805 (cap 4000000)`, followed by **352** `Failed to upload LOD terrain block … scene geometry admission is closed after reaching the resident pool limit` and object-LOD equivalents, while `Exterior LOD rings settled around (-17,0)` is still logged. At `--radius 3` the same route logs 0 admission failures. One `SLOW FRAME dt=20241 ms` also occurred at radius 12 (plus 3.1 s and 1.6 s frames); the 20 s frame is not attributed and may have coincided with console calls on the 106k-entity world.
- **Impact**: after the default door exit the player gets full detail to 12 cells and no distant terrain or object LOD beyond it. Admission stays latched closed until unload compaction reopens it, and the reconcile reports "settled" anyway.
- **Suggested Fix**: reserve pool headroom for the LOD rings, or cap the full-detail radius by pool pressure; make "settled" fail when uploads are rejected.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix

