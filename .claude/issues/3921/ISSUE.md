# #3921: FO3-2026-09-05-D5-01: `ragdoll_import.rs` claims to cover "every classic-chain game" and has no FO3 arm

Filed from `docs/audits/AUDIT_FO3_2026-09-05.md` (FO3-2026-09-05-D5-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `low,game:fo3,legacy-compat,physics,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3921 --json state`.

---

**Source**: `docs/audits/AUDIT_FO3_2026-09-05.md` (FO3-2026-09-05-D5-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: 5 — Collision / PHYSAL ingestion
- **Location**: `crates/nif/tests/ragdoll_import.rs` — module doc plus the four
  `#[ignore]` tests (`fnv_humanoid_skeleton_threads_ragdoll`,
  `oblivion_humanoid_skeleton_threads_ragdoll`, `skyrim_humanoid_skeleton_threads_ragdoll`,
  `fnv_protectron_skeleton_is_one_connected_component`).
- **Status**: NEW.
- **Description**: The file's own doc says a humanoid ragdoll "must thread end-to-end
  into `ImportedScene.ragdoll` through the *same* code path on **every** classic-chain
  game (Oblivion / FNV / Skyrim)". FO3 is a classic-chain game — `docs/engine/physal.md`
  lists Oblivion/FO3/FNV/Skyrim as converged — and it is silently absent from the list.
  `crates/nif/src/import/collision/ragdoll.rs` gained 266 lines in this delta with no
  FO3 arm anywhere to catch a regression.
- **Evidence**: measured this run — FO3's
  `meshes\characters\_male\skeleton.nif` threads **17 authored → 17 surfaced
  constraints, 1 connected component, 18 bodies**, identical to FNV's
  `meshes\characters\_male\skeleton.nif` (17 → 17, 1 component, 18 bodies). So the code
  path is correct on FO3; only the gate is missing.
- **Impact**: An FO3-only ragdoll regression (or an FO3 DLC skeleton divergence) would
  go undetected. Low, because FO3 and FNV currently agree exactly — but that agreement
  is the thing nothing checks.
- **Related**: `#3792` (Prismatic joins the surfaced-kind tally), `docs/engine/physal.md`.
- **Suggested Fix**: Add `fo3_humanoid_skeleton_threads_ragdoll` reusing
  `thread_skeleton_ragdoll(Game::Fallout3, r"meshes\characters\_male\skeleton.nif")` and
  `assert_structural`; the harness already skips cleanly when data is absent.

**Otherwise verified clean.** The `constraint_drift_corpus` gate passes
(4 069 decoded-constraint drift events, all known motor-tail values). No collision
extraction failure was observed. The prior report's correction stands: FO3 authors
**zero** `bhkMultiSphereShape`, so the #1277 MultiSphere half is unfalsifiable here;
the `BhkConvexListShape` half is FO3-reachable and was verified on 2026-08-30.

*Not covered this run*: the full 19 229-chain `CollisionAuthoring::Classic` census was
not re-run — the delta in `collision/shape.rs` is 29 lines and the corpus parse gate
(which walks every block of every FO3 NIF) shows no new failure outside `NiSkinPartition`.

---

### Dimension 6 — BSA v104 + real-data validation

**Verified clean. No new findings.**

- All six FO3 mesh-bearing archives open and enumerate:
  `Fallout - Meshes.bsa` 14 167 files, `Anchorage - Main` 7 199, `BrokenSteel - Main` 7 785,
  `PointLookout - Main` 8 177, `ThePitt - Main` 11 674, `Zeta - Main` 4 326.
  Extraction of all 17 172 NIF entries succeeded (0 extraction failures; the 294
  truncations are parser-side, not archive-side).
- `Fallout - Textures.bsa`: **12 261 DDS entries, every header parsed** (width/height/
  mip count decoded). 1 166 carry a mip chain shorter than
  `floor(log2(max(w,h)))+1` — that is Bethesda's authored mip truncation, present in the
  shipped data, not a reader defect. Matches the prior 12 261 count exactly.
- FO3 has no `<stem>N.bsa` sibling family (unlike FNV's `Textures2.bsa`), so the
  `asset_provider/archive.rs` auto-load rule is inert here; the #3637/#3896
  last-listed-wins inversion that reshuffled FNV's `Update.bsa` ordering has no FO3
  analogue — FO3 ships no base-game patch archive and its profile lists a single mesh BSA.
- BSA v104 itself is shared with FNV; `crates/bsa/tests/bsa_real.rs` covers v104
  through `fnv_meshes_bsa_v104_extracts_nif_with_gamebryo_magic`. Divergence here would
  be a v104 regression, not an FO3 format gap — none observed.

*Scope note*: the FO3 profile in `assets/debug_profiles.toml` declares no
`default_scripts_bsas` / `default_sounds_bsas` and no DLC masters or DLC archives.
That is a deliberate base-game profile, consistent with the other pre-FO4 entries;
not reported as a finding.

---

### Dimension 7 — FO3 animation / NPC spawn + scripting gap

**Verified clean. No new findings.** All three M41.0 long-tail guards hold on FO3:

- **#772 B-spline pose fallback** — the `FLT_MAX` sentinel gate is present at four
  sites in `crates/nif/src/anim/bspline.rs` (translation, rotation, scale, and the
  fallback `NiQuatTransform`). `9ce6b7a5` ("finite-guard the B-spline dequantised
  sample path") did not disturb the FO3 corpus: measured **3 572 / 3 572 `.kf` files
  parsed, 3 608 clips, 0 zero-clip files**, exactly the 2026-08-30 baseline, carrying
  5 347 486 translation / 8 965 437 rotation / 275 462 scale keys.
- **#790 `AnimationClipRegistry` dedup** — `CanonKey::Owned(key.to_ascii_lowercase())`
  is intact, so case-variant paths still intern to one clip and FO3 exterior streaming
  cannot leak a keyframe set per cell load.
- **#793 / M41-HANDS** — `body_paths_kf_era_include_separate_hand_meshes` explicitly
  iterates `[GameKind::Oblivion, GameKind::Fallout3NV]` and asserts all three of
  `upperbody.nif` / `lefthand.nif` / `righthand.nif`; the gender/child variants are
  pinned separately for `Fallout3NV`. **The resolver is correct — but see D2-01: both
  hand meshes now fail to parse their skin partition, so the meshes this guard loads
  arrive degraded.** The guard is a path-resolution test and cannot see that.
- **Scripting gap (unchanged, not a bug to file)**: FO3's 1 257 SCPT records parse and
  still have no runtime that executes them. `attach_scpt_script`'s dialect match gives
  `CharacterRulesProfile::FALLOUT3` no compiled-bytecode `ObscriptDialect` (only
  Oblivion gets `Obse`, only FNV gets `Xnvse`), and `obscript_runtime.rs` only compiles
  from preserved `SCTX` source for the script-extender load-order idiom vanilla FO3
  never authors. This remains the largest FO3-specific functional gap and is owned by
  `/audit-scripting`.

---

### Dimension 1 — FO3 rendering path (inline shaders)

**Verified clean. No new findings.** This is the largest FO3-vs-FNV divergence surface
and it absorbed real churn (`material_translate.rs` +629, `components/material.rs` +446,
`legacy_properties.rs` +94, `shader_flags.rs` +21, new `dedicated_shader.rs`), so each
checklist item was re-derived rather than assumed:

- **FO3 does not route through the Skyrim slot table.** `apply_pp_lighting_property`
  binds `BSShaderTextureSet` slots 0–5 through its own explicit arms
  (base / normal / glow / parallax / env / env-mask). `slot_to_role` — whose
  `TextureSlotLayout::from_bsver` would label FO3 (`bsver` 34) as `Skyrim` — is reached
  only from the REFR texture-overlay path in `cell_loader/spawn/mesh_instance.rs`, and
  FO3 authors zero overlays (#3511). The Skyrim slot-2 `glow_map` gate (#3068) therefore
  never applies to FO3, which is correct: nif.xml's `BSShaderFlags2` has no `Glow_Map`
  bit at all for FO3/FNV. *(Observation, not filed: an FO3 `ImportedMaterial` still
  carries `texture_slot_layout == Skyrim`, which is factually wrong but has no live
  consumer on this title.)*
- **Flag layout is the legacy single-u32 pair.** `crates/nif/src/shader_flags.rs`
  keeps `fo3nv_f1` / `fo3nv_f2` separate from `skyrim_slsf*` and `fo4_slsf*`, with
  pinned tests for every same-bit-different-meaning collision that matters here:
  bit 22 (`TREE_BILLBOARD` vs `OWN_EMIT`), F2 bit 21 (`ALPHA_DECAL` vs
  `ANISOTROPIC_LIGHTING`), and bits 15/16 (`REFRACTION`/`FIRE_REFRACTION`, genuinely
  shared). No FO4 u32-pair leaks in.
- **`EmissiveSource` discriminator holds.** Full-archive census of
  `Fallout - Meshes.bsa` (10 989 NIFs): **2 321 authored-emissive meshes, 100 % in the
  `mat` bucket** — `EmissiveSource::Material`. Zero `Lighting`, zero `Effect`. The
  multiplier distribution is FO3-shaped (1 334 at exactly 1.00, tail to 20.0; 10.9 %
  at ≥ 10), so the Skyrim+/FO4 variants are not bleeding into the FO3 ≈1.0 scale.
- **Disney-BSDF gate is structurally unreachable.** Every production writer of
  `ImportedMaterial::is_pbr = true` lives in `byroredux/src/asset_provider/material.rs`
  (the BGSM / BGEM / Starfield `.mat` merge arms). `crates/nif/src/import/` pins it
  `false` at both construction sites. FO3 authors no external material file, so
  `MAT_FLAG_PBR_BSDF` cannot be set — the remaining `is_pbr = true` sites in
  `cell_loader.rs` are inside `#[cfg(test)] mod pack_imported_material_flags_tests`.
- **`NoLighting` fullbright route intact.** `apply_no_lighting_property` still tags
  `material_kind = 102` (`MATERIAL_KIND_NO_LIGHTING`) so `triangle.frag` short-circuits
  the lit path, and shares `is_decal_from_legacy_shader_flags` with the PPLighting arm
  so the `ALPHA_DECAL` F2 bit-21 check (#454) stays in lockstep.
- **Fire-refraction promotion (`material_kind = 103`)** remains a monotone latch, with
  `refraction_strength` now `_consumed`-gated (#3514, landed `0cb5eed9`) — the last bare
  `=` in `apply_pp_lighting_property`. Unreachable on vanilla FO3 (0 of 17 172 NIFs bind
  a `BSShader*` on a `NiNode`), so this is consistency for modded content.
- **NIFAL single boundary verified by the harness**: `cross_game_translation_completeness`
  reports FO3 at 687 meshes, `mat_path = 0.0 %` (no external material — as expected),
  `metO`/`rghO` 94.3 % resolved, **`consistent = 100.0 %`**, all fill-rate floors passed.
- **Typed particle emitters reach FO3.** `real_archive_torch_meshes_surface_particle_emitters`
  reports FO3 at 137 emitters across 5 meshes with `params = 137`, `rate = 132`,
  `budget = 137`, and the per-game magnitude floors added by #3754 pass — so the
  `float_interpolator_rate` time-weighted-mean fix is still live on FO3 data.

---

## FNV-Shared Surface

Everything FO3 inherits from the FNV path was exercised this run and is healthy
**except** the one shared parser defect:

| Shared mechanism | FO3 status |
|---|---|
| ESM parser (`crates/plugin/src/esm/`) | ✅ every FO3 baseline exact |
| CELL walker + `PGRE`/`PROJ` (#3542/#3753) | ✅ fixed and gated |
| `NiTexturingProperty` v20.1.0.3+ flags branch (#2565, #3621, #3623) | ✅ FO3 is `V20_2_0_7` → takes the unchanged `Flags` arm; the widened dark/detail/gloss/glow reads did not disturb the FO3 corpus |
| `NiSkinPartition` strip de-strip | 🔴 **D2-01** — shared regression, 294 FO3 files / ≥ 741 FNV files |
| `havok_motion_type` (#1652) | ✅ unchanged |
| B-spline import | ✅ 3 572/3 572 `.kf`, 0 zero-clip |
| BSA v104 reader | ✅ |
| Weather / CLMT resolution | ✅ (`NAM0` #533, `DNAM` #534 gates pass) |

D2-01 is reported here with FO3 evidence because FO3 is where it was *proven*; it
belongs to the shared parser and the FNV audit will see its own numbers. It is one
defect, not two.

---

## FO3-Distinctive Gaps (unchanged, informational)

1. **Inline-shader-only material universe.** FO3 never authors `BSLightingShaderProperty`
   or BGSM. Confirmed unreachable, not merely unobserved (Dim 1 above).
2. **Capital Wasteland worldspace.** Distinct WRLD; handled by EDID preference. No
   FNV-hardcoded coordinate found.
3. **1 257 SCPT records with no executing runtime.** M47.0 event hooks and M47.1
   condition eval exist; the M47.2 recognizer slice does not reach FO3's dialect. Known
   blocker for FO3 quest/world interactivity, owned by `/audit-scripting`.
4. **`GameKind::Fallout3NV`** collapses FO3 and FNV — no code can express an FO3-only
   behaviour. Design constraint, not a defect.

---

## Validation Status

| Tier | Status |
|---|---|
| NIF parse (all 6 archives) | ⚠️ 100 % recoverable, **98.29 % clean** — 294 truncated, regression D2-01 |
| Block dispatch | ⚠️ 526 109 blocks, **296 `NiUnknown`** (all `NiSkinPartition`) |
| ESM (`Fallout3.esm`) | ✅ every baseline exact, cell tier + mine chain gated |
| Interior (Megaton) | ✅ parse-side 929 REFR baseline unchanged (`MegatonPlayerHouse`); **no live load performed** |
| Exterior (Capital Wasteland) | ✅ wired, worldspace selection correct; **fresh GPU bench still pending (R6a-stale-15)** |
| Creature / NPC skinning | 🔴 296 skin partitions lost, incl. both humanoid hand meshes |
| Animation (`.kf`) | ✅ 3 572/3 572, 3 608 clips, 0 zero-clip |
| Ragdoll | ✅ 17/17 constraints, 1 component, 18 bodies — **ungated (D5-01)** |
| Textures (DDS) | ✅ 12 261 headers parse |
| Materials (NIFAL) | ✅ 100 % consistent, all floors passed |

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_FO3_2026-09-05.md
```

Label every finding `game:fo3` + `legacy-compat`, plus its own domain label
(`nif-parser` + `nif` for D2-01, `test-gap` for D2-02/D5-01, `esm-plugin` +
`test-gap` for D3-01). **Publish D2-01 first and fix it before anything else in this
report** — it is a one-line change (`allocate_vec_sized` → `allocate_vec_min_bytes(…, 2)`),
it is already independently diagnosed in `AUDIT_NIF_2026-09-04.md` with no issue trace,
and it is currently dropping vanilla content on three shipped titles.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
