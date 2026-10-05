# #5285: NIFAL-D9-2026-10-05-01: #5102's declared mesh→medium boundary lists six beam converters; `CachedNifImport::beam_volumes` chains seven, and the FO4 lamp-shaft converter is the one left out

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5285
- **Labels**: low,nifal,documentation,doc-rot,game:fo4
- **Source**: `docs/audits/AUDIT_NIFAL_2026-10-05.md` (NIFAL-D9-2026-10-05-01)

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW · **Dimension**: Completeness · **Tier Violated**: doc / record-keeping · **Game Affected**: FO4
- **Location**: `docs/engine/nifal.md:137-143` versus `byroredux/src/cell_loader/nif_import_registry.rs:389-405`;
  `byroredux/src/fog.rs:1376` (`fo4_ambient_lamp_beam_volume_from_mesh`).
- **Status**: NEW (incomplete fix of #5102, closed by `cb0aaf58c`)
- **Description**: The spec names `fnv_nellis_hangar_beam_volumes_from_mesh`, `window_…`, `oblivion_dungeon_…`,
  `authored_cone_…`, `fnv_superwide_…` and `vault_window_beam_volume_from_mesh`. The chain's seventh arm,
  `fo4_ambient_lamp_beam_volume_from_mesh`, replaces FO4 `effects\ambient\` emergency, fluorescent and dusty lamp-shaft submeshes.
  It includes `emergencylightbeam01.nif` (242/1080), the very signature the #5102 source finding cited. Its media constants
  (extinction clamped 0.001–0.3, `edge_softness` 0.5) are also absent from the doc's constant list.
- **Impact**: The declared inventory under-reports which titles lose drawn submeshes on the cell path. It omits FO4 entirely, and
  FO4 is the largest population.
- **Related**: #5102, #4809, `docs/engine/interior-godrays-status.md`.
- **Suggested Fix**: Add the seventh converter and its constants. Consider a doc-pin test that counts the `beam_volumes` chain arms
  against the named list.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
