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

### F3 — FO4 static collision absent (HIGH — scoped 2026-05-26, fix is multi-day work)

```
M28.5 NO STATIC COLLIDERS in the Rapier world — every body is Dynamic/Kinematic.
Cell has no parsed bhk static architecture.
```

FO4 cells aren't producing static collision shapes. Without them, the player phases through floors / walks through walls in every FO4 interior.

**2026-05-26 root-cause investigation:**

- FO4 architecture NIFs (e.g. `meshes\interiors\institute\main\insmainbasefloor01.nif`) reference collision via **`bhkNPCollisionObject`** — the "NP" (new physics) variant Bethesda introduced for FO4 — not the classic `BhkCollisionObject`.
- The NP collision object points to a **`bhkPhysicsSystem`** block that contains a Havok-serialised binary blob (`ByteArray`) carrying the actual shape tree.
- Our parser correctly identifies and reads both as raw bytes via [`BhkSystemBinary.data: Vec<u8>`](../../crates/nif/src/blocks/collision/collision_object.rs#L135-L154) — the blob is captured intact.
- **But `extract_collision()` at [`crates/nif/src/import/collision.rs:26-73`](../../crates/nif/src/import/collision.rs#L26-L73) only handles `BhkCollisionObject → BhkRigidBody → shape tree`** — the classic FNV/FO3/Skyrim chain. It returns `None` immediately for `bhkNPCollisionObject`, so every FO4 architecture REFR produces zero collision data.
- The precombined `_oc.nif` files would normally bake the architecture collision into a cell-level mesh, but those NIFs' geometry lives in the deferred `Fallout4 - Geometry.csg` companion (#1188) — also not parsed today. So the precombined fallback also yields zero collision.

**Fix surface (NOT in this session — multi-day):**

1. Implement a Havok content-system binary deserialiser that reads `BhkSystemBinary.data` and reconstructs the shape tree. Reference impls: `nifly`'s `bhkPhysicsSystem` handler (C++, ~2k LOC), OpenMW does NOT cover FO4 collision.
2. Or, partial coverage: detect `bhkNPCollisionObject` → look up its `bhkPhysicsSystem` → match a small set of common shape signatures (Box / Capsule / TriMesh) by byte-pattern probing the Havok blob. Cheap-but-fragile; only useful as a triage step.
3. Long-term: integrate the CSG companion reader (#1188) so the precombined mesh + collision both materialise.

**Affected scope**: every FO4 interior cell, every FO4 exterior worldspace. FO76 / Starfield use the same NP physics so the same fix unlocks them.

**Workaround**: until then, FO4 cells render correctly but have no playable physics. M28.5 character controller has nothing to ground against → falls indefinitely. Not a regression of recent work — this gap predates the symptom-sweep findings.

### F4 — FO4 NIF parse failure rate 4× the others (FIXED 2026-05-27)

| Game | Failures / Total | Rate |
|---|---|---|
| Oblivion | 0 / 52 | 0.0% |
| FNV | 1 / 200 | 0.5% |
| FO3 | 3 / 270 | 1.1% |
| Skyrim | 11 / 296 | 3.7% |
| **FO4 pre-fix** | **15 / 182** | **8.2%** |
| **FO4 post-fix** | **0 / 182** | **0.0%** |

**Root cause**: not a parser failure — a cell-loader **gating** failure. The
`BSXFlags` bit-5 universal gate in `parse_and_import_nif`
(`byroredux/src/cell_loader/references.rs:873-876` pre-fix) classified
every NIF with bit 5 set as an editor marker and returned `None`. But
**bit-5 semantics changed across game eras**:

- **Oblivion / FO3 / FNV** (BSVER < `FALLOUT4`): bit 5 = `EditorMarker`. Filter is correct.
- **Skyrim / FO4 / FO76 / Starfield** (BSVER >= `FALLOUT4`): Bethesda re-purposed bit 5 as `MultiBoundNode` (a culling hint, NOT editor-only). Filter was wrongly dropping legitimate architecture.

In `InstituteBioScience`, 15 NIFs had bit 5 set and got filtered. The list:

- 3 floors: `hitfloorsolidfull01`, `hitfloorsolidmid01`, `hitfloorsolidmid01a` (visible architecture, 3 meshes each)
- 2 doors: `insdoorsm01` (11 meshes), `inssecuritydoor01` (5 meshes)
- 2 dust light-beams + 1 klaxon glow
- 7 editor markers (correctly invisible — they have 0 meshes regardless)

**Fix** at [`references.rs:870-906`](../../byroredux/src/cell_loader/references.rs#L870-L906):
make the bit-5 gate game-aware by re-parsing the NIF header for
`user_version_2` (BSVER) and only filtering when `bsver < FALLOUT4`. FO4+
editor markers are still filtered by the name-based check at
[`walk/mod.rs:1430`](../../crates/nif/src/import/walk/mod.rs#L1430)
(`is_editor_marker` matches `editormarker*` / `marker_*` / `markerx` /
`marker:*` / `mapmarker*` — every shipping FO4 editor-marker NIF
authored a name in that family).

**Verified**: post-fix `mesh.cache failed` reports "No failed NIF parses
in cache." Entity count 7552 → 7758, unique meshes 987 → 1015, textures
317 → 326 on InstituteBioScience.

**`mesh.cache failed` console subcommand** added at
[`commands.rs:486-512`](../../byroredux/src/commands.rs#L486-L512) — lists
every cached NIF path whose parse returned `None`. Was the decisive
diagnostic for finding F4's actual failure set.

### F5 — FNV/FO3 base-record dispatch gaps (RECLASSIFIED — mostly benign 2026-05-27)

- **FNV**: `6 base forms not found in statics table`
- **FO3**: 1 missing base (`00000021`)
- **Oblivion / Skyrim**: 0

**2026-05-27 investigation outcome — these are NOT statics; the cell-loader log message is misleading.**

Probing each FormID via the new `probe_form` example
([`crates/plugin/examples/probe_form.rs`](../../crates/plugin/examples/probe_form.rs))
which walks all 10 indexed record categories:

| FormID | Game | Category | EditorID | User impact |
|---|---|---|---|---|
| `001055B6` | FNV | **ACTI** | `VCG01DocMitchellCouchTrigger` | Invisible quest trigger volume — should NOT render a mesh |
| `00105BCC` | FNV | **ACTI** | `VCG01DocMitchellChairTrigger` | Same |
| `00107232` | FNV | **ACTI** | `GSDocMitchellExitTrigger` | Same |
| `00104C07` | FNV | **ACTI** | `VCG01VigorTesterTrigger` | Same |
| `001046FB` | FNV | (not in any indexed category) | — | Likely SOUN or other unparsed record type |
| `001046FC` | FNV | (not in any indexed category) | — | Same |
| `00000021` | FO3 | (engine-defined) | `Player` | The player-placement marker, NOT an ESM record — expected miss |

**4 of the 6 FNV cases are ACTI quest-trigger volumes** that correctly should not render a mesh. The "missing base" log line is misleading because the cell loader only does a STAT-table lookup, doesn't categorise the REFR as "ACTI → spawn as invisible volume." The mesh-load is correctly skipped; the diagnostic just looks alarming.

**Functional impact**: zero today. The 4 ACTI triggers would fire scripts if the script-execution path (M30+) were online — and at that point we'd want to route the REFR through `ACTI` category lookup to attach an `Activator` ECS component, not a mesh.

**Proposed log-message improvement** (1-line change, not in this session):

Replace the current "base forms not found in statics table" warning with a multi-category probe — "base forms not found in any category (or missing from parser dispatch)" so the message reflects the actual gap. Today's wording implies a parser bug that isn't there.

**Tracked**: when script execution lands, extend cell-loader REFR dispatch to walk `index.activators` / `index.containers` / `index.doors` / `index.npcs` and spawn the appropriate ECS component. Today this would surface 4 of the 6 FNV entries as proper Activator entities.

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
