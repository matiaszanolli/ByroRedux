# #3735: SPT-2026-08-30-D3-01: 0 of 154 vanilla .spt TREE records resolve — extract_mesh builds a meshes\ key but the archives keep .spt under a top-level trees\ folder, so the whole SpeedTree subsystem is unreachable

**Labels**: bug, import-pipeline, high, legacy-compat, game:fnv, game:fo3, game:oblivion, terrain-exterior, speedtree
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-30.md` · **Severity**: HIGH · **Dimension**: 3 (TREE→Billboard Wiring)
**Game affected**: Oblivion, Fallout 3, Fallout NV

> **Independently corroborated**: a sibling `/audit-regression` run this same session reached the same conclusion by a different route.

## Location
- `byroredux/src/cell_loader/references/synth_child.rs` — the `model_path` composition and the `is_spt` dispatch downstream of `tex_provider.extract_mesh(&model_path)`
- `byroredux/src/asset_provider/archive.rs` — `normalize_mesh_path`
- `byroredux/src/asset_provider/texture.rs` — `extract_mesh`

## Description
The `.spt` dispatch is downstream of `tex_provider.extract_mesh(&model_path)`. **That lookup never succeeds for a vanilla TREE record, on any of the three `.spt` games**, so the `is_spt` arm is never entered: control takes the `None` arm, logs `"SPT not found in BSA"` at debug level, increments `nif_not_found`, and the REFR is dropped.

The entire Session-33 Phase 1 subsystem — walker, importer, billboard quad, `SpeedTreeWind`, and **#3528's freshly-landed ICON resolver** — is unreachable on shipped content.

This is the same defect class as #3528 (a Bethesda path convention the engine's single generic normaliser does not model) **one step earlier in the chain**, which is why #3528's guard did not catch it: `vanilla_tree_icons_all_resolve` pins ICON resolution, and nothing pins the MODL that feeds `extract_mesh`.

## Evidence
Measured this run over `FalloutNV.esm` / `Fallout3.esm` / `Oblivion.esm` and the four vanilla mesh archives.

**MODL path shape** — every `.spt`-bearing TREE record, zero exceptions:

| Game | `.spt` TREE records | leading-separator, no directory | has a directory | bare filename |
|---|---:|---:|---:|---:|
| Oblivion | 142 | **142** | 0 | 0 |
| FO3 | 9 | **9** | 0 | 0 |
| FNV | 3 | **3** | 0 | 0 |

Samples: `\Dbush16.spt`, `\ShrubVineMapleSU.spt`, `\WhiteOak01.spt`, `\OasisElm02.spt`, `\Pine01.spt`.

**Where the archives actually keep them** (folder/file-record walk): `Oblivion - Meshes.bsa` v103 → 113 `.spt`, all under `trees\`; `Fallout - Meshes.bsa` v104 (FO3 and FNV) → 10 `.spt`, all under `trees\`. Note the folder is `trees\`, **not** `meshes\trees\` — SpeedTree binaries live outside the `meshes\` root in all three games.

**The key the engine builds**, traced through the three sites:

```
synth_child.rs   model_path = "meshes\" + "\WhiteOak01.spt"
                            = r"meshes\\WhiteOak01.spt"
archive.rs       normalize_mesh_path sees the "meshes\" head -> Cow::Borrowed
bsa/mod.rs       normalize_path lowercases, '/'->'\\' (does not collapse "\\")
                 -> lookup key r"meshes\\whiteoak01.spt"
archive holds    r"trees\whiteoak01.spt"                       => MISS
```

**Resolution rate**, simulating that exact key against the real file tables:

| Game | `.spt` TREE records | resolve with the current key | resolve as `trees\<name>` |
|---|---:|---:|---:|
| Oblivion (incl. Shivering Isles archive) | 142 | **0** | **142** |
| FO3 | 9 | **0** | **9** |
| FNV | 3 | **0** | **3** |
| **total** | **154** | **0** | **154** |

## Impact
**No SpeedTree placeholder billboard has ever rendered from a cell load on vanilla FNV, FO3 or Oblivion.** Cyrodiil exteriors — which lean entirely on TREE REFRs for forest content — are treeless, and both Fallouts lose their `.spt` vegetation.

Degradation is graceful (the cell loads, the REFR is skipped, nothing panics), which is why the symptom reads as "content gap" rather than "bug" and has survived every prior cycle. Blast radius is the whole `crates/spt` public surface plus #994/#997/#1000/#1001/#1002/#3528/#3529 — all correct in isolation, none of them observable. The loose `--tree` route is unaffected (the user supplies the archive-internal path directly), which is exactly why the smoke path passes while the production path does not.

## Related
#3528 (same class, one step later, ICON); #1711. The `Cyrodiil pines` framing in #1001 and #1002 presumes trees reach the screen at all. The BNAM/MODB sizing finding filed alongside this one becomes visible the moment this is fixed.

## Suggested Fix
Give the SpeedTree route the same treatment #3528 gave ICON — a `.spt`-scoped resolver beside `resolve_tree_icon_path` that strips a leading separator and probes `trees\<name>.spt` before falling back to the authored value, **keeping `normalize_mesh_path` (shared by every mesh consumer) untouched**. Pin it with an env-gated corpus guard mirroring `vanilla_tree_icons_all_resolve` — assert that all 154 vanilla TREE MODLs resolve — and confirm the `trees\` root against the archives rather than hardcoding it from this report.

## Completeness Checks
- [ ] **SIBLING**: `normalize_mesh_path` already carries a `geometries\` exemption for the same class of defect (#1292) — check whether any other non-`meshes\` root is unmodelled
- [ ] **CANONICAL-BOUNDARY**: path resolution belongs in the asset provider, not in the SpeedTree importer or the render path. See `/audit-nifal`.
- [ ] **TESTS**: an env-gated corpus guard asserting all 154 vanilla TREE MODLs resolve, mirroring `vanilla_tree_icons_all_resolve`
