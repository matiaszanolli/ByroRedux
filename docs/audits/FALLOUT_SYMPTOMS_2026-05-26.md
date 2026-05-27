# Fallout Series — Current Visual / Functional Gap Inventory

**Date**: 2026-05-26
**Method**: Headless telemetry sweep (Xvfb-hosted engine + `byro-dbg` console queries) across one representative interior per game. Counters: `stats`, `tex.missing`, `tex.loaded`, `mesh.cache`, `skin.coverage`, engine WARN/ERROR log. Baseline cells confirmed to populate (Oblivion / Skyrim are the "gorgeous" baselines per visual review).

## Per-Game Numbers

| Game | Cell | Entities | Loaded tex (unique) | Fallback tex | NIF parse fails | WARN count |
|---|---|---:|---:|---:|---:|---:|
| Oblivion | `ICMarketDistrictTheGildedCarafe` | 696 | 82 | **0** | 0/52 = 0% | 19 |
| Skyrim SE | `WhiterunDragonsreach` | 5 885 | 145 | 1 | 11/296 = 3.7% | 18 |
| FNV | `GSDocMitchellHouse` | 1 634 | 156 | **54** | 1/200 = 0.5% | 7 |
| FO3 | `MegatonPlayerHouse` | 3 278 | 196 | **44** | 3/270 = 1.1% | 8 |
| FO4 | `InstituteBioScience` | (timed out on stats) | 91 | **21** | **15/182 = 8.2%** | **51** |

The Fallout column on `Fallback tex` is **20–50× higher than Oblivion/Skyrim**. That's the real signature of the gap. FO4 separately has a 4× elevated NIF parse-fail rate and 3–7× the WARN volume.

---

## Findings, Ordered by Leverage

### F1 — FO4 NPC skeleton path lookup fails universally (HIGH)

Every FO4 NPC in `InstituteBioScience` (~20 NPCs, ~30 WARNs total) hits the same path:

```
NPC <FormID>: skeleton 'meshes\actors\character\character assets\skeleton.nif' not in archives — skipping mesh spawn (equip state retained)
```

The path contains a space (`character assets`) and is the canonical FO4 shared NPC skeleton. If a single path-normalisation or BSA-lookup case isn't handling the embedded space (or the file is in `Fallout4 - Meshes.ba2` under a slightly different keying), **every NPC in every FO4 interior renders as floating equipment** because the spawn skips when the skeleton isn't found.

- **Affected scope**: every FO4 cell, every NPC.
- **Likely fix surface**: BSA lookup code at `byroredux/src/asset_provider.rs` or the path-resolve step in `crates/bsa/src/archive/`.
- **Quick triage step**: grep `Fallout4 - Meshes.ba2` listing for `skeleton.nif`, confirm exact path including separator chars; compare against the lookup-side normalisation.

### F2 — Fallout NIF importer leaves Material empty for ~50 entities per cell (RECLASSIFIED — NOT a simple bug, see investigation 2026-05-26)

Per-cell `<no path, no material>` counts: **FNV 54, FO3 44, FO4 19, Oblivion 0, Skyrim 1**.

For these entities the diagnostic is *worse than missing-texture*: the engine doesn't even know which texture path was supposed to be there. The Material component is fully empty (no `texture_path`, no `material_path`), `TextureHandle == 0` (fallback / checker), and the entity renders as chrome.

**2026-05-26 investigation outcome — this is NIF-authored, not a parser bug.**

Walked one fallback entity from FNV `GSDocMitchellHouse` end-to-end:

- Entity 41 (`cabinet:5`, sub-shape of REFR `0x104C13`'s `nv_vitomaticvigortester_cabinet.nif`)
- Sibling sub-shapes `cabinet:0`, `cabinet:3`, `cabinet:6` all have proper textures (`NV_Vigor-Tester_Cabinet1.dds`)
- `cabinet:5` alone has `AlphaBlend(src=6, dst=7)`, `material_kind=0`, all texture slots empty
- Dumped the NIF's 22 `BSShaderNoLightingProperty` blocks: block 166 (the one bound to cabinet:5) has **empty `file_name`** — authored that way by the artist
- The walker correctly extracted the empty string and `intern_texture_path("")` correctly returned `None`

So the importer is faithful. The artist authored the shape with no texture (it's a glass overlay; alpha-blend without an albedo source). Bethesda's renderer presumably uses vertex colour or the material's emissive/diffuse term as the surface look. **Our renderer falls through to the magenta-checker fallback because `TextureHandle == 0` is treated as "missing file" universally**, with no distinction between "file not in archive" and "file not authored." Same root cause for:

- alpha-blend overlays with no albedo (cabinet:5 / glass panels)
- emissive-only halos (`nvcraftsmanrmcorinwindowr01b:10` has `emissive_mult=10` but no texture — light-bulb glow)
- vertex-colour-driven shapes (vcm=2 across all three sampled entities)
- anim-only NIFs that incorrectly produce a renderable entity (`headanims:0` — possible bug, small surface)

**Root-cause class**: renderer-side. The fragment shader / fallback sampler doesn't honour "no texture authored" as a distinct state from "texture missing from archive."

**Proposed renderer-side fix surface** (not in this session):
1. Detect at spawn-time (or in `build_render_data`): if `TextureHandle == 0` and `Material.texture_path.is_none()` and `Material.material_path.is_none()`, mark the entity as `NoTextureAuthored` (new marker component or a flag in `Material`).
2. Fragment shader: when `NoTextureAuthored` is set, skip the texture sample entirely; use vertex colour × diffuse × emissive directly as the albedo. The magenta checker is only used when an *authored path* failed to resolve.

**Diagnostic improvements landed in this session** (not the fix, but make future passes faster):
- `tex.missing entities` lists entity IDs per bucket (5 samples per path), so the next pass can `mesh.info` immediately.
- `mesh.info` now dumps full Material shape: `material_kind`, alpha state, `emissive_mult`, `effect_shader_flags`, `env_map_scale`, `vertex_color_mode`, plus marker components (`AlphaBlend`, `TwoSided`, `IsFxMesh`, `RenderLayer`, `SceneFlags`, `DoorTeleport`).
- `crates/nif/examples/dump_nolighting.rs` lists every `BSShaderNoLightingProperty.file_name` and `BSShaderPPLightingProperty` texture-set linkage — for verifying "is this empty in the NIF itself?"

### F3 — FO4 static collision absent (HIGH)

```
M28.5 NO STATIC COLLIDERS in the Rapier world — every body is Dynamic/Kinematic.
Cell has no parsed bhk static architecture.
```

FO4 cells aren't producing static collision shapes. Either bhk dispatch is mis-routing FO4-era collision blocks, or FO4 stores collision in an external format (`.hkx`?) we don't parse yet. Without static collision, the player can fall through floors / walk through walls in every FO4 interior.

- **Affected scope**: every FO4 cell.
- **Fix surface**: `crates/nif/src/blocks/collision/*` dispatch + `crates/plugin/src/esm/cell/` static-collider plumbing.

### F4 — FO4 NIF parse failure rate 4× the others (MEDIUM)

| Game | Failures / Total | Rate |
|---|---|---|
| Oblivion | 0 / 52 | 0.0% |
| FNV | 1 / 200 | 0.5% |
| FO3 | 3 / 270 | 1.1% |
| Skyrim | 11 / 296 | 3.7% |
| **FO4** | **15 / 182** | **8.2%** |

8.2% is 4× the Skyrim baseline and 8× FO3. FO4-specific NIF block types or BSVER dispatch arms are failing.

- **Already filed**: `audit-fo4` reports from 2026-05-15/18 surface findings around `BSLightingShaderProperty.Backlight Power` gate inversion (FO4-D1-NEW-01) — possibly the cause of part of these failures.
- **Triage**: enable per-NIF parse-failure logging, group failures by block-type / BSVER.

### F5 — FNV/FO3 base-record dispatch gaps (LOW)

- **FNV**: `6 base forms not found in statics table (sample: 001055B6, 00105BCC, 00107232, …)` — 6 REFRs in `GSDocMitchellHouse` point to base records that the parser didn't catalogue.
- **FO3**: 1 missing base (`00000021`).
- **Oblivion / Skyrim**: 0.

These are REFRs pointing to base record types the ESM walker doesn't dispatch yet for Fallout (possibly DLC-only types or pre-Skyrim record variants). The REFRs silently spawn with no base mesh.

### F6 — `assets/debug_profiles.toml` sample_cells are wrong (LOW, tooling)

- `[profiles.fo3].sample_cells` lists `"Megaton01"` — does not exist. Correct: `Megaton` (worldspace) or `MegatonPlayerHouse` (interior).
- `[profiles.fo4].sample_cells` lists `"DiamondCityMarket"` — does not exist. Diamond City is an *exterior* worldspace (`DiamondCity`, form `00000F94`). Pick a real interior like `InstituteBioScience`.
- `[profiles.fo4].sample_cells` lists `"InstBioscience01"` — does not exist. Correct: `InstituteBioScience` (singular, no `01` suffix).

These caused two of the five sweep runs to silently load an empty default scene with 6 entities. One-line fixes in `assets/debug_profiles.toml`.

### F7 — Scheduler duplicate exclusive system warning (cosmetic, all 5 games)

```
Scheduler: duplicate exclusive system name 'byroredux::App::new::{{closure}}' in stage Update
(prefer `try_add_exclusive` for named struct systems)
```

5× per launch, in *every* game. Not Fallout-specific, but consistent log-noise. The suggested fix is in the warning itself.

### F8 — Skyrim FaceGen NIFs with zero meshes (LOW)

```
NIF 'meshes\actors\character\facegendata\facegeom\skyrim.esm\00087b95.nif' imported with zero meshes
```

2 NPC FaceGen heads in Whiterun Dragonsreach. Already tracked as **#1225**; logged with the importer-bug hypothesis already.

---

## What's NOT a Symptom (sanity check)

- Oblivion: 0 fallback textures, 0 parse failures, 0 missing-base. The "gorgeous baseline" is gorgeous because the pipeline is genuinely clean for v20.0.0.5 content.
- Skyrim SE: 1 fallback (engine-internal `<no path>` sentinel), 11 parse fails out of 296 (3.7% — concerning if Skyrim were the target, but well below Fallout-series fail rates).
- TLAS missing-BLAS warnings: **0** across all 5 games (the `SKIN_MAX_SLOTS` bump from this morning held).
- WorldBound seed inserts: confirmed plumbed via Oblivion's pick-able scene (`pick` returns real authored radii, not synthetic fallbacks).

---

## Recommended Sequence

| Order | Finding | Why first | Estimated leverage |
|---|---|---|---|
| 1 | **F1** (FO4 NPC skeleton path) | Single path-normalisation fix unblocks every NPC in every FO4 cell | Highest — "FO4 NPCs are visible" is a single boolean |
| 2 | **F6** (profile sample_cells) | One-line fixes; immediately fixes diagnostic ergonomics | Highest per-effort |
| 3 | **F2** (Fallout Material plumbing gap) | Common to FNV + FO3 + FO4 — find the one shader-property walker arm that's leaking | High — fixes 54+44+19 = 117 chrome-checker entities across three games |
| 4 | **F3** (FO4 static collision) | Player phasing through walls is high-noticeability | High; bigger fix surface (likely needs bhk dispatch work) |
| 5 | **F4** (FO4 NIF fail rate) | Some overlap with **F2**; ride that fix's coattails | Medium |
| 6 | **F5** (FNV/FO3 base dispatch) | Small surface; mostly drops "ghost REFR" placeholders | Low/Medium |
| 7 | **F7** (scheduler warning) | Easy log-hygiene | Cosmetic |
| 8 | **F8** (Skyrim FaceGen) | Already tracked at #1225 | Already in queue |

---

## Reproducer

```bash
# /tmp/telemetry_sweep.sh + /tmp/telemetry_sweep2.sh
# Per-game outputs at /tmp/telemetry-sweep/{engine,telem}-<key>.{log,txt}
```

Tools used:
- `xvfb-run` + `cargo run --release --bench-frames 30 --bench-hold` for headless engine launch
- `cargo run -p byro-dbg` with stdin-piped commands for telemetry harvest
- `pkill` cleanup between game launches

The sweep takes ~5 min total wall time and is rerunnable as a regression check after any Fallout-targeted fix.
