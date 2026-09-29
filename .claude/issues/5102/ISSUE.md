# NIFAL-D1-2026-09-29-02: mesh → FogVolume beam/fog replacement runs on the cell path only and is an unrecorded drawn-surface exemption

**Labels**: low,bug,nifal,renderer,terrain-exterior,game:oblivion,game:fo3,game:fnv,game:fo4

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-29.md`
**Severity**: LOW
**Dimension**: Material (drawn-surface boundary) / Completeness
**Tier Violated**: single-boundary (load-path divergence); no-fabrication (weak sense)
**Game Affected**: Oblivion, FO3, FNV, FO4
**Location**:
- `byroredux/src/cell_loader/nif_import_registry.rs`: `CachedNifImport::beam_volumes`, which chains the asset-signature converters in `byroredux/src/fog.rs` (`fnv_nellis_hangar_beam_volumes_from_mesh`, `window_beam_volume_from_mesh`, `oblivion_dungeon_beam_volume_from_mesh`, `authored_cone_beam_volume_from_mesh`, `fnv_superwide_beam_volume_from_mesh`, `vault_window_beam_volume_from_mesh`);
- `byroredux/src/cell_loader/spawn/mesh_instance.rs` (`prepare_fog_mesh_instance` → `fog::fog_volume_from_mesh`, and the `FogGroup` path);
- `byroredux/src/scene/nif_loader.rs` calls none of them.

## Description
`0572bfd5a` added six asset-signature converters, each matching a file name plus exact vertex/index counts (e.g. `NVNellisHangarInteriorLightBeam.nif` 110 verts / 120 indices; FO4 `emergencylightbeam01.nif` 242/1080; Oblivion `lightbeam01.nif` 14/36). On a match the drawn submesh is **replaced** before upload: no `Material`, no raster entity, no BLAS; one or more canonical `FogVolume`s are emitted instead. The older fog-token converter (`fog_volume_from_mesh`, `733dff8f1`) does the same for `dst_blend == 7` fog/smoke quads.

This is a NIF-data → canonical-type translation with no declared boundary in `nifal.md` §2. `nifal.md` §3 says drawn-surface exemptions are "exactly three" (Cornell, save, ground cover); this would be a fourth. The media parameters are uncited: extinction 0.12 m⁻¹, albedo 0.9, `edge_softness` 0.35–0.65, an 80-BU beam half-width, extrusion of span × 0.3 or width × 0.75. (A painted card has no extinction to translate, so this is weak-sense no-fabrication, not invented data.)

## Evidence
`rg 'beam_volume_from_mesh|beam_volumes_from_mesh|fog_volume_from_mesh' byroredux/src` finds only `fog.rs`, its tests, `nif_import_registry.rs` and `mesh_instance.rs`.

## Impact
The same NIF renders two ways: `cargo run -- effects/ambient/windowlightbeam.nif` draws the painted card, while that NIF placed in a cell becomes a medium. The real game path (cells) is unaffected; the main cost is that the population is invisible to the §3 exemption ledger and the NIFAL boundary inventory.

## Related
EXT-D1-02 / EXT-D4-0x (`AUDIT_EXTERIOR_2026-09-27.md`, godray lighting side), #4809 (beam classification caching), `docs/engine/interior-godrays-status.md`. `fog.rs` is owned by exterior/renderer; this is the NIFAL facet only.

## Suggested Fix
- Record "mesh → participating medium (cell path)" in `nifal.md` §2/§3 as a declared boundary, citing the converters and the status doc.
- Either route the loose-NIF spawn through `beam_volumes` / `fog_volume_from_mesh`, or state the viewer divergence as deliberate.
- Cite or measure the media constants.

Validated at HEAD 9fcfdc3fc: `CachedNifImport::beam_volumes` chains the six beam converters; `nifal.md` §3 still says the exemption list "has exactly three"; `scene/nif_loader.rs` has no call to any beam/fog-from-mesh converter.

## Completeness Checks
- [ ] **SIBLING**: other cell-path-only mesh replacements (e.g. particle → medium `medium_from_particle`) checked for loose-NIF parity
- [ ] **CANONICAL-BOUNDARY**: any routing change keeps the translation at spawn/import, never at render time
- [ ] **TESTS**: a pin that both spawn paths agree (or that the divergence is documented)
