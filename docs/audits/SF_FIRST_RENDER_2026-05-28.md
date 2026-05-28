# Starfield First Render — Cydonia, 2026-05-28

**Cell**: `CityCydoniaMainLevel` (form `0x002B3DA2`, vanilla `Starfield.esm`)
**Engine**: HEAD `7c263a93` (post-Phase-0+1 Starfield ESM work)
**Tool**: `cargo run --release` with `--esm Starfield.esm --cell citycydoniamainlevel --bsa "Starfield - Meshes01.ba2" --textures-bsa "Starfield - Textures01.ba2" --materials-ba2 "Starfield - Materials.ba2" --bench-hold`
**Log**: [cydonia-2026-05-28.engine.log](sf-first-render/cydonia-2026-05-28.engine.log) (15 029 lines, archived for grep)

## Result

| Stage | Status | Numbers |
|-------|--------|---------|
| ESM parse | ✓ | 1.36 GB read in 4 s, zero panics |
| TES4 + master detection | ✓ | `GameKind::Starfield` auto-detected from HEDR 0.96 |
| Cell index | ✓ | `CityCydoniaMainLevel` resolved by EDID |
| Cell-record decode | ✓ | 27 898 placed REFRs extracted |
| **XCLL cell lighting** | ⚠ | 108-byte size warned non-canonical 11 985× (one per interior cell). Decodes partial fields out of wrong offsets. **Files [#1291](https://github.com/matiaszanolli/ByroRedux/issues/1291).** |
| Material provider | ✓ | Starfield CDB loaded (97 classes, 1.44 M instances) per [#1289](https://github.com/matiaszanolli/ByroRedux/issues/1289) closeout |
| BA2 mesh archive | ✓ | `Starfield - Meshes01.ba2` (4.0 GB) opened, extraction works |
| **NIF mesh import** | ✗ | **1 624 of 27 898 REFRs (5.8 %) silently drop to "zero meshes"** — but those 1 624 represent the load-bearing categories (SetDressing 1 040, Architecture 403, …). Per-REFR count: scene reports only **75 entities spawned** from the cell — **99.73 % spawn-rate failure**. **Files [#1292](https://github.com/matiaszanolli/ByroRedux/issues/1292).** |
| NIF parser correctness | ✓ | 147 minor block-size-drift warnings on BSEffectShaderProperty / BSLightingShaderProperty, all recovered |
| Vulkan render | ✓ | Engine reached `Engine ready — entering game loop`, ran at 175 FPS / 5.70 ms / 0 GPU draw calls |
| Telemetry | ✓ | byro-dbg attached, full `stats / tex.missing / mesh.cache failed / light.dump` cycle succeeded |

## What the user sees

A black void. Engine is healthy, FPS is high (because there's nothing to render), camera moves, no panics — but the geometry simply isn't there. Only 3 unique mesh handles ever get registered, and 0 of them are in active use at the time of telemetry capture.

## Where the gap is

**Not in the ESM parser.** The ESM dispatches Starfield content end-to-end at 99.9 % parity (confirmed by Phase 1's `sf_parse_check`).

**Not in the NIF format reader.** All 27 898 referenced NIFs parsed without `?`-bailout errors.

**Not in the Vulkan renderer.** The pipeline accepts whatever it gets; it just got essentially nothing.

**The gap is the NIF → ImportedMesh path for SF content** ([#1292](https://github.com/matiaszanolli/ByroRedux/issues/1292)). Starfield uses BSGeometry blocks that store vertex data in external `.mesh` companion files (the FO76+ pattern). The 2026-05-18 audit reported the path as "fully wired"; the runtime says 99.7 % of Cydonia REFRs hit it and produce no geometry. Likely root cause is the same parser-landed-consumer-unwired pattern that [#1289](https://github.com/matiaszanolli/ByroRedux/issues/1289) closed for the CDB consumer — different layer, same shape.

The XCLL size mismatch ([#1291](https://github.com/matiaszanolli/ByroRedux/issues/1291)) is a SEPARATE, smaller gap: every interior cell's lighting parameters are being read out of the wrong subrecord offsets. It's a one-day fix; SF-NIF-01 is a multi-day investigation.

## What works really well

The ESM parser performed magnificently:

- HEDR auto-detection
- Master file detection (`["Starfield.esm"]` correctly handled — no Constellation/DLCs loaded for this test)
- 11 985 interior cells + 18 424 exterior cells + 432 worldspaces decoded
- 41 620 STAT-family base objects (architecture / furniture / lighting / decals)
- 27 898 REFRs in the target Cydonia cell with positions / rotations / scales / base-form-id refs
- Cell lighting subrecord at least PARTIALLY decoded (6-axis cube + fog params surfaced via byro-dbg)
- CDB consumer (#1289 Phase 1) loaded the 97-class / 1.44M-instance material database without panic

The Phase 0+1 effort estimate revision was correct: the ESM workstream collapsed from "build a parser" to "fix one cell-lighting size table." The actual visible-render blocker is on the NIF side, where it always was — we just didn't know it until the cell loaded.

## Next concrete actions

1. **Fix [#1291](https://github.com/matiaszanolli/ByroRedux/issues/1291)** (XCLL 108-byte size). One-day mechanical fix. Eliminates 11 985 log-spam warnings + corrects every SF cell's lighting.
2. **Investigate [#1292](https://github.com/matiaszanolli/ByroRedux/issues/1292)** (NIF geometry drop). Start with `nif_stats` on `meshes\SetDressing\Posters\FFCydoniaZ04_SpaceFrog_Poster_01.nif` — a simplest-possible textured-plane case that should be trivial to render. If THAT NIF doesn't extract a quad, the root cause is in the BSGeometry external-mesh path, not in some obscure shader-block decoder.

After both close, re-render Cydonia and measure how the entity count and `tex.missing` distribution change. The Phase 5 success criteria (≥50 REFRs, FPS > 30) is already met technically (75 entities, 175 FPS) — but the *visible* success criterion is "Cydonia looks like Cydonia," which needs SF-NIF-01.

## Files referenced

- Roadmap: [docs/engine/starfield-esm-roadmap.md](../engine/starfield-esm-roadmap.md)
- Phase 0+1 baseline: [docs/engine/starfield-esm-phase0-baseline.md](../engine/starfield-esm-phase0-baseline.md)
- Archived log: [sf-first-render/cydonia-2026-05-28.engine.log](sf-first-render/cydonia-2026-05-28.engine.log)
- Filed issues: [#1291](https://github.com/matiaszanolli/ByroRedux/issues/1291), [#1292](https://github.com/matiaszanolli/ByroRedux/issues/1292)
- Closed sibling (CDB consumer pattern): [#1289](https://github.com/matiaszanolli/ByroRedux/issues/1289)
