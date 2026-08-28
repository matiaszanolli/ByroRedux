# #3466 — NIF-2026-08-27-D3-02: the corpus gates walk 34.8% of FO76, 70.8% of FO4 and 74.1% of Starfield — and that is exactly where both new parser defects live

Source: `docs/audits/AUDIT_NIF_2026-08-27.md`
Filed: 2026-08-27 via `/audit-publish`
Labels: medium, nif-parser, nif, bug, test-gap, game:fo76, game:fo4, game:starfield

---

Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 3 (Block Dispatch Coverage, test infrastructure). Severity **MEDIUM**. Games: **Fallout 76, Fallout 4, Starfield**.

## Location
`crates/nif/tests/common/mod.rs:141-190` (`Game::mesh_archives`), `crates/nif/tests/parse_real_nifs.rs:203` (`parse_rate_starfield_all_meshes`), `crates/nif/tests/parse_real_nifs.rs:286` (`parse_rate_fo4_all_meshes`).

## Description
`Game::mesh_archives()` lists a single archive for each of Fallout 4 (`Fallout4 - Meshes.ba2`), Fallout 76 (`SeventySix - Meshes.ba2`) and Starfield (`Starfield - Meshes01.ba2`). Two dedicated all-meshes tests widen FO4 to two archives and Starfield to five. FO76 has no widening test at all. The FO3 and FNV lists, by contrast, already enumerate every DLC archive.

## Evidence
NIF-entry counts per archive (`is_nif_entry`, i.e. `.nif` + `.bto` + `.btr`), measured this sweep:

| Game | Gated | Vanilla/official total | Coverage | Largest omissions |
|------|------:|-----------------------:|---------:|---|
| Fallout 76 | 58,469 | 168,220 | **34.8%** | `GeneratedMeshes01` 20,245 · `StaticMeshes` 17,334 · 16 × `*UpdateMain` ≈ 70k · `GeneratedMeshes02` 2,049 |
| Fallout 4 | 166,568 | 235,141 | **70.8%** | `DLCCoast - Main` 34,411 · `DLCNukaWorld - Main` 27,511 · `DLCRobot - Main` 3,647 · 3 × `DLCworkshop*` 2,945 |
| Starfield | 89,276 | 120,543 | **74.1%** | `LODMeshesPatch` 19,540 (the sibling of the *gated* `MeshesPatch`) · `ShatteredSpace - Main01` 9,198 · 6 × `SFBGS* - Main` 2,529 |

This is not a hypothetical gap. Every one of the 112,716 `BSDistantObjectExtraData` `NiUnknown` blocks (NIF-2026-08-27-D3-01) and 135 of the 1,417 `BSFaceGenNiNode` drift events (NIF-2026-08-27-D1-01) sit outside the gates; the `unknown_ceiling_fallout_76` baseline reads `unknown_blocks 0` and passes, because `SeventySix - Meshes.ba2` genuinely has zero.

Widening is safe and free: the two all-meshes tests already `continue` cleanly on a missing archive (`parse_real_nifs.rs:238-241`), and every omitted archive parsed in this sweep came back clean — FO4's four DLC archives are 173,160/173,160 clean with zero drift and zero `NiUnknown`; Starfield's `LODMeshesPatch` is 19,540/19,540 clean.

## Impact
The structural regression guard this domain relies on ("extend, don't reinvent") is blind to two thirds of FO76's shipped NIF corpus and to every FO4/Starfield DLC. A dispatch or drift regression confined to that content lands green.

## Related
Closed #3041 (the same blind spot, closed for FNV); open #3369 (Skyrim SE — neither covers FO76/FO4/Starfield); #3150 (committed scratch probes in `crates/nif/examples/`).

## Suggested Fix
Add a `parse_rate_fo76_all_meshes` sibling covering `Meshes` + `MeshesExtra` + `StaticMeshes` + `GeneratedMeshes01/02` + the `*UpdateMain` set; extend `parse_rate_fo4_all_meshes` with the six DLC `Main.ba2`s (mirroring the FO3/FNV lists, which already do this); extend `parse_rate_starfield_all_meshes` with `LODMeshesPatch.ba2` and `ShatteredSpace - Main01.ba2`. Regenerate the `block_coverage_baselines` ceilings **after** D3-01 (#3461) lands, not before, or the FO76 ceiling bakes in 112,716.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`Game::mesh_archives` per-game lists, `block_coverage_baselines`, `per_block_baselines`, the #3369 Skyrim SE sibling)
- [ ] **TESTS**: A regression test pins this specific fix
