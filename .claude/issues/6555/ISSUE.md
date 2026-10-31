# OBL-D3-2026-10-31-01: #6459 stopped unlit VFX sheets from synthesizing solid colliders on FO3/FNV, but Oblivion's cobwebs and FX sheets still pass `render_geometry_can_collide`

Labels: high,nifal,physics,legacy-compat,bug,game:oblivion

Filed from `docs/audits/AUDIT_OBLIVION_2026-10-31.md` (finding `OBL-D3-2026-10-31-01`).

- **Severity**: HIGH. Same impact class as FNV-2026-10-27-02 (filed HIGH, fixed by #6459); not confirmed on a device (suite rule).
- **Dimension**: 3 (spawn colliders). Mechanism owner: `/audit-nifal` and the spawn collision fallback; sibling of FNV-2026-10-31-05, which is the same predicate on FNV content.
- **Location**: `byroredux/src/cell_loader/spawn.rs:94-103` (`render_geometry_can_collide`), `byroredux/src/cell_loader/spawn/mesh_instance.rs:1526-1530` (the `ArchitectureTriMesh` site), `crates/nif/src/import/material/legacy_properties.rs:835-860` (the only writer of `MATERIAL_KIND_NO_LIGHTING`).
- **Status**: NEW for Oblivion. The 10-27 report recorded this as an Oblivion leg of FNV-02 and said "the `MATERIAL_KIND_NO_LIGHTING` key does not cover Oblivion". #6459 chose exactly that key.
- **Description**: The new predicate excludes `material_kind == MATERIAL_KIND_NO_LIGHTING && has_alpha`. That kind (102) is assigned only from `BSShaderNoLightingProperty`, which Oblivion does not have. Oblivion VFX sheets and cobwebs are plain `NiAlphaProperty` blend-only meshes with `material_kind == 0`, no alpha test, no decal, no effect shader, so every term of the predicate passes and a STAT base with no `bhk*` goes through the missing-collision fallback and gets a `Fixed` trimesh.
- **Evidence**:
  - Probe over `Oblivion - Meshes.bsa` (`/mnt/data/tmp/oblprobe/src/bin/vfx.rs`, `import_nif`, predicate re-evaluated with the #6459 terms): `cobweb01`..`07`, `hangcrud01`, `fxgasplane01`, `fxmistgroundeffect01` (and the other FX bases probed) all print `bhk=0 kind=0 has_alpha=true alpha_test=false decal=false effect=false => can_collide=true`.
  - `Oblivion.esm` census (Python over STAT/REFR, 2026-10-31 re-run): 19 STAT bases (`cobweb0*`, `HangCrud01`, `FXLightBeam*`, `FXCloudSmall01`, `FXMistGroundEffect01`, `FXWhiteTorchLarge`, `LightBeam01`, `FXGasPlane01`, `FXOblivionLightBeam*`) with **8,949 placements**, **6,906** of them cobwebs and `HangCrud01`. Unchanged from the 10-27 count.
- **Impact**: Zero-thickness solid sheets hang across dungeon corridors and over open ground: invisible walls for the player capsule, NPC walks and dropped bodies. `DUNHay`, `BloodDecal01/02` and `ARWelkydClusterFX01` share the same signature but are different cases and need their own look.
- **Suggested Fix**: key on the dialect-independent canonical signal #6459 intended, not on NoLighting: a blended (`has_alpha`), non-alpha-tested mesh with `material_kind == 0` and no authored collision should not be a body. Check whether the importer carries a `z_write` or blend flag usable here; the cobweb and FX set above is the pin (`--ignored`, over `Oblivion - Meshes.bsa`, like the FNV test #6459 added). Register it in `.github/real-data-pins.txt`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
