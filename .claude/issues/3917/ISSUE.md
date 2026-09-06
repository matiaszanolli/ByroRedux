# #3917: FNV-2026-09-05-D2-01: `SoundArchiveProvider::extract` is first-listed-wins while #3637 made mesh / texture / material last-listed-wins, and unlike `ScriptProvider` it carries no rationale for the difference

Filed from `docs/audits/AUDIT_FNV_2026-09-05.md` (FNV-2026-09-05-D2-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `low,game:fnv,legacy-compat,audio,import-pipeline,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3917 --json state`.

---

**Source**: `docs/audits/AUDIT_FNV_2026-09-05.md` (FNV-2026-09-05-D2-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: 2 — asset-provider precedence (adjacent to the NIFAL boundary
  audit; not a `translate_material` defect)
- **Location**: `byroredux/src/asset_provider/audio.rs` —
  `SoundArchiveProvider::extract`; contrast
  `byroredux/src/asset_provider/texture.rs` — `TextureProvider::extract` /
  `extract_mesh` / `extract_mesh_exact` and
  `byroredux/src/asset_provider/material.rs`, all of which iterate `.rev()`
- **Status**: NEW
- **Description**: #3637 (`3562401b`) inverted mesh/texture/material archive
  lookup from first-wins to **last-wins**, matching Bethesda load-order
  precedence, and #3896 then had to re-order `[profiles.fnv]`'s `default_bsas`
  because the old ordering had silently become backwards. `SoundArchiveProvider`
  was not part of that sweep: it still iterates `for archive in &self.archives`
  and documents "First-listed archive wins on a collision" with no reason given.
  `ScriptProvider` is *also* first-wins, but deliberately so and says why (#1743
  / SCR-D7-03, cited in its doc); the audio provider has no such anchor, so the
  divergence reads as an oversight rather than a decision.
- **Evidence**: `grep -n 'for archive in' byroredux/src/asset_provider/*.rs`
  → five `.iter().rev()` sites (texture ×4, material ×2) and one forward
  iteration (`audio.rs`). `ScriptProvider::resolve_pex`'s doc names #1743;
  `SoundArchiveProvider::extract`'s does not.
- **Impact**: **Zero on FNV today** — `[profiles.fnv]`'s `default_sounds_bsas`
  is a single archive, so no collision can occur. It becomes real the moment
  the four DLC `* - Sounds.bsa` archives are added (the natural next step after
  #3788), or on `--game skyrim_se`, which already supplies **two**
  (`Skyrim - Sounds.bsa` + `Skyrim - Voices_en0.bsa`) and therefore already runs
  a precedence rule opposite to its own mesh and texture pools.
- **Related**: #3637, #3896, #1743, #3788.
- **Suggested Fix**: Either flip `SoundArchiveProvider::extract` to `.rev()` for
  consistency with #3637, or keep first-wins and record why in the doc the way
  `ScriptProvider` does. Silent divergence between three sibling providers is
  the part worth removing.

---

## Regression Guard List — verified at `d10c8433`

Measured against real FNV data unless marked *(code)*.

| Guard | Evidence |
|---|---|
| **NIF parse rate** | **20 746 `.nif` + 5 014 `.kf` = 25 760 files, 0 hard parse failures**, across all 11 mesh-bearing archives; per-archive counts byte-exact vs. ROADMAP |
| **#3541 normal synthesis — FNV is unaffected** | 48 504 imported shapes in `Fallout - Meshes.bsa`: 2 936 carry a uniform `[0,1,0]` normal field, **0 of them over faces that disagree with world-up**. The 2 663 `landscape\lod` NIFs alone: 2 975 shapes, 38 uniform-up, **0 with relief**. FNV authors normals where Oblivion/Skyrim/FO4 LOD does not, so the new `synthesize_normals_or_default` path is a no-op here |
| **#1538** SCOL is FNV-era | `is_scol_era = is_fo4_plus \|\| Fallout3NV` intact; **98 SCOL** measured *(code + data)* |
| MOVS / PKIN / MSWP FO4+-only | **0 / 0 / 0** across 465 017 records |
| **#1654** SCHR flags are a `u16` | **2 576 of 2 576** FNV SCHR are exactly 20 bytes; `script.rs` still reads the tail with `u16_or_default` |
| CELL `XCLL` optional fields | **388 of 388 XCLL are 40 bytes**; `LGTM.DATA` 31 × 40 B — `fog_far_color` correctly lands `None` |
| **#3511/#1887** XATO is FO3/FNV's Activation Prompt | **219 XATO** (ACTI 59 / REFR 156 / ACHR 3 / ACRE 1), **121 distinct prompt strings**. The walker's un-game-gated FormID read is documented as a "near-certain inert miss" — measured **0 of 219 collide with any of FNV's 493 TXST FormIDs**, so the misparse is provably inert, not merely probably |
| `XTNM` / `XTXR` have no FNV corpus | **0 / 0** |
| Record-type dispatch coverage | **108 of 108** FNV record types reach a parser |
| **#1652** Havok motion type | Full enum intact (`1..=5\|8`→Dynamic, 6→Keyframed, 7→Static, 9→CharacterKinematic); the `4 ⇒ Keyframed` collapse has not returned *(code)* |
| **#1269** `MAX_NIF_NODE_DEPTH = 128` | Present on both walkers (`walk/mod.rs`); no FNV import trips it |
| **#1539/#1718/#1540/#1772/#3330/#3792** ragdoll drop family | **All 61 FNV skeletons surface 100 % of authored constraints**; 59 in one connected component, `protectron` **12/12 / 1 component** (#3792 holding), both sentry-turret skeletons 3/3. The two 5-component outliers (`skeletonmale50scasual`, `skeletonfemale50scasual`) drop **zero** constraints — pre-dismembered corpse props, matching last cycle's stale-candidate #2 |
| **#3792's open census — now answered** | Across all 11 archives: `bhkBallAndSocketConstraint` **0**, `bhkStiffSpringConstraint` **1** (`meshes\dlc04\effects\dlc04slimebubble03.nif`, an FX mesh). Live constraint mix: LimitedHinge 597 · Ragdoll 708 · Malleable 143 · Hinge 4 · Prismatic 3 · Breakable 1. The `BhkConstraintData::Other` drop arm has **one** FNV occupant and it is not a skeleton |
| PHYSAL single seam | `grep GameKind` finds **zero** matches in `crates/physics/src/` and `byroredux/src/ragdoll.rs` *(code)* |
| **#1873/#2553/#2573** no fabricated metalness | `classify_pbr_keyword`'s `!inputs.specular_authored` early-return intact; `resolve_pbr` now forwards `self.specular_authored` (#2573) instead of hardcoding `false`; the FNV-specific `has_legacy_bs_shader && specular_color == [0;3]` un-authoring (#2553) still runs before the `specular_enabled` zeroing *(code)* |
| **Disney BSDF gate is unreachable on FNV** | `MAT_FLAG_PBR_BSDF` is set only from `ImportedMaterial::is_pbr`, whose only writer is `merge_external_material`. Measured: **0 `.bgsm` / `.bgem` / `.mat` files across all 20 FNV BSAs** — the lobe cannot be reached |
| NIFAL single boundary | `every_exterior_spawner_inserts_a_boundary_material` is no longer a hand-maintained file list — it now scans `cell_loader/*.rs` for `MeshHandle(` and requires a boundary call (#3733), with a ≥6 floor. `cell_loader/water.rs` joined the covered set *(code)* |
| EmissiveSource no bleed | FNV authors no `BSLightingShaderProperty` / `BSEffectShaderProperty`; only `NiMaterialProperty` can write, → `EmissiveSource::Material` |
| **#1125** skyTint interior gate | Present at **all three** miss fallbacks: `raytrace.glsl::traceReflection`'s hoisted `missCol`, and both `isExteriorGlass ? … : sceneFlags.yzw` sites in `triangle.frag`. #3323's `exteriorSkyTint` read is a *different*, deliberately-scoped consumer (the window-portal branch) and does not weaken the gate |
| **#1799** legacy WRS off | `ENABLE_LEGACY_WRS 0` in `shader_constants.glsl`; `RESTIR_M_CAP = 20.0` *(code)* |
| `8b5d77c1` sun-sprite mip 0 | `composite.frag`'s sun-sprite fetch is `textureLod(…, 0.0)` *(code)* |
| **#1520** unload releases BLAS + Rapier | `accel.drop_blas(mh)` per freed handle, `release_victim_rapier_bodies` → `pw.remove_ragdoll`, plus the skin/morph slot cleanup; all four test siblings present *(code)* |
| **#3415** exterior `LoadedCellIndex` | `cell_loader/index.rs`'s source-scan test still asserts **both** the interior loader and `assemble_exterior_streaming` insert it *(code)* |
| `NifImportRegistry` Arc cache | `CachedNifImport` still held as `Option<Arc<…>>` behind `ParsedNifCache`, with `snapshot_keys` handing the stream worker an `Arc<HashSet>` *(code)* |
| **#3321** object-LOD blocks | `object_lod_archive_path`'s `FalloutLegacyBlocks` arm and `object_lod_atlas_path`'s `<world>.buildings.dds` both resolve: **555 block quads across 26 worldspaces** (295 for `wastelandnv`), **24 atlas pairs** present in `Fallout - Textures2.bsa`. `LodBandLadder::for_object_game` still routes `Fallout3NV` to `fallout_legacy()` |
| **#3548** interior-`XCLW` zero gate | FNV: **388 interiors author XCLW; 341 are `#INT_MIN#` and 8 are FLT_MAX**, both filtered at the parser boundary by `xclw_water_height` (#1305); of the **39** that survive, **zero** are `0.0`. The #3548 doc table's FNV row is exactly right, and `interior_water_height` is a correct no-op here |
| **WTHR / WATR / CLMT layout** | FNV `WTHR.DATA` is **15 bytes on all 63** records (parser gates `>= 15`, Skyrim's 19 handled separately); `CLMT.TNAM` 6 B × 31; `WATR.DATA` **70 × 2 B damage stubs + 8 × 186 B** visual payloads, both arms present |
| **#3637/#3896** archive precedence | `extract` / `extract_mesh` / `extract_mesh_exact` / the FaceGen fallback / both material lookups all iterate `.rev()`. FNV impact measured: `Fallout - Textures.bsa ∩ Textures2.bsa` = **0 keys**, so the inversion is a no-op for textures; `Meshes.bsa ∩ Update.bsa` = **36 keys**, where last-wins now correctly gives `Update.bsa` |
| **#340/#790/#772/#793** animation family | Lowercased canonical clip keys (`registry.rs::CanonKey`), FLT_MAX sentinel on all three B-spline channels, hand-mesh assertions present. All seven hardcoded FNV NPC asset paths verified present in the archives (`skeleton.nif`, `upperbody`/`childupperbody`/`childfemaleupperbody`/`femaleupperbody`, `lefthand`/`righthand`, `idleanims\chairskirt_leftenter.kf`, `locomotion\mtidle.kf`) |
| Typed emitters | `extract_emitter_params` / `extract_emitter_rate` still feed `walk/mod.rs`'s `ImportedScene` from the typed `NiPSysEmitter*` structs *(code)* |
| **#3255** AI-package handover | `clear_ambient_behavior` now also removes `NavPath`, closing the stale-path reuse across a package switch *(code)* |
| Seven procedure runtimes opt-in | All seven env gates (`BYRO_SANDBOX_SIT`/`WANDER`/`TRAVEL`/`FOLLOW`/`ESCORT`/`GUARD`/`PATROL`) present in `boot.rs`; none of the seven systems is registered unconditionally. `ambient_ai_package_system` (M42.9 selection, not a procedure runtime) is unconditional by design |
| M42.2 CTDA fail-open | `package_conditions_pass` not flipped to fail-closed *(code)* |
| **#1305** XCLW sentinel filter | `xclw_water_height`'s symmetric `\|h\| < 1e9` + `is_finite` gate covers both `#INT_MIN#` and FLT_MAX; **349 of FNV's 388** interior XCLW depend on it |

---

## Stale candidates dropped (3)

1. **"341 FNV interiors get a water plane at `y = -2147483648`."** The raw
   bytes really do say that: 341 of FNV's 388 XCLW-bearing interiors author
   `#INT_MIN#` and 8 author FLT_MAX, and #3548's new `interior_water_height`
   filters only `0.0`. Dropped after reading the *parser* boundary:
   `xclw_water_height` (#1305, `esm/cell/helpers.rs`) already rejects both
   sentinels with a symmetric `|h| < 1e9` test before `interior_water_height`
   ever sees the value. The post-filter count is exactly the **39** #3548's doc
   table claims. Premise falsified end-to-end.
2. **"REFR `XATO` misparsed as a TXST FormID could collide with a real
   texture set."** The walker's own comment calls the collision "near-certain
   miss" — which is a probability claim, not a measurement. Dropped after
   measuring: **0 of 219** XATO payloads' leading `u32` matches any of the 493
   FNV TXST FormIDs, and none is shorter than 4 bytes. Closed #1887's "benign"
   verdict is now data-backed rather than argued.
3. **"`crates/nif/src/import/mesh/skeleton.rs` (446 new LOC) touches FNV
   skinning."** Dropped on reading it: the module resolves external-skeleton
   bone names for Starfield `BSSkin::Instance` blocks with NULL `bone_refs`
   (#3549). It has no reachable arm on the `NiSkinInstance` path FNV uses.
   Likewise `crates/plugin/src/esm/records/pathgrid.rs` (303 new LOC) is
   Oblivion `PGRD` — **FNV ships 0 PGRD** (it has 4 771 NAVM + 1 NAVI instead).

---

## Already tracked — cited, not re-filed

- **#2367** (OPEN) — the `3a02b02d..28155b79` performance regression, which
  names Prospector. Not re-measured this cycle (no device).
- **#3816** (OPEN) — REGN ambient music decode for Skyrim/Oblivion.
  FNV-2026-09-05-D1-01 above is about its FNV *gap*, not its content.
- **#3301** (OPEN) — REGN incidental spatial emitter + non-Sound RDAT
  selectors. `RDSD` (76 on FNV, SOUN-typed) still deliberately unsurfaced.
- **#3189** (OPEN per prior audio cycles) — `try_load_default_water_splash`
  re-opens the sound archive. Same function as FNV-2026-09-05-D8-01, different
  defect: #3189 is about *how* it opens, D8-01 is about *what it looks for*.
- **#3866 / #3872** (OPEN, filed today) — the `compute_blas_budget` →
  `blas_budget_for_heap` / `probe_blas_heap_bytes` rename (`fa5c4191`) and its
  doc fallout. `.claude/commands/audit-fnv/SKILL.md`'s Dimension-3 bullet was
  already corrected in the working tree during this run.
- **#3843, #3855, #3856, #3857** (OPEN, filed today) — oversized-file findings
  covering `extensions.rs`, `boot.rs`, `walk/mod.rs` and
  `asset_provider/material.rs`, all of which this audit read through.

---

## Not investigated (needs a live device)

Deliberately not asserted here — no engine was launched (standing rule against a
parallel launch, and a sibling audit held the GPU):

- The Prospector Saloon and WastelandNV bench-of-record numbers, and therefore
  any FPS/fence/draw regression claim.
- TLAS frustum culling dropping no in-view light; shadow-ray budget caps;
  distance-based shadow/GI fallback.
- SVGF / TAA behaviour under real camera motion; M33 sky-gradient blend against
  tone-mapped geometry.
- `tex.missing` / `tex.loaded` console output on a loaded FNV cell.
- The *visual* half of the FNV object-LOD ring: paths, atlas and ladder are all
  confirmed resolvable against the real archives, but nothing here proves the
  555 block quads render.
- The `--ignored` corpus gates (`per_block_baseline_fallout_nv`,
  `parse_rate_fnv_esm`, `translation_completeness`, `normal_synthesis_corpus`).
  The independent measurements above cover their *headline* numbers but not the
  per-block "0 unknown blocks" claim, which only the checked-in baseline can
  settle.

One coverage note the shared protocol asks for: of the eight un-owned
subsystems in `_audit-common.md`, this audit touched the launcher profile
surface (`assets/debug_profiles.toml`, `game_profiles.rs`) only where FNV
depends on it, and did not examine `crates/sdk`, `crates/mod-runtime`,
`byroredux/src/extensions.rs`, `crates/facegen`, `crates/hkx`, or the debug
server.

---

## Suggested Next Step

`/audit-publish docs/audits/AUDIT_FNV_2026-09-05.md`

Label every finding `game:fnv` + `legacy-compat`, plus its own domain label:
`audio` for D8-01 and D4-01, `esm-plugin` for D4-01 and D1-01,
`import-pipeline` for D8-02 and D2-01. D8-02 and D2-01 are `bug`/`low`;
D1-01 is closer to `documentation`/`low` (a tracking correction, not a code
change).

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
