# Starfield Compatibility Audit — 2026-08-20

*Run as part of the `comprehensive` audit-suite sweep (25 audits, 335 commits
since 2026-08-16). All 9 dimensions covered. Real game data at
`/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` (129 archives,
89,276 vanilla NIFs, the 1,389 MB `Starfield.esm`).*

**Method note.** Per the suite briefing, `cargo build` / `cargo test` /
`cargo check` were **not** run and the engine was **not** launched. Every
real-data number below was produced by independent out-of-tree readers written
for this pass (a Python BA2 v1/v2/v3-GNRL/DX10 reader, a NIF header +
block-type + block-size walker, and an ESM GRUP/record/sub-record walker),
so they cross-check the Rust implementation rather than restating it. Two
dimensions that can only be exercised through `cargo` (`--sf-smoke` resolve
rate, `parse_rate_starfield_all_meshes`) are carried, not re-measured — see
*Coverage Gaps*.

---

## Executive Summary

Starfield remains a first-class `GameKind`. **All five findings the 2026-08-16
pass raised were fixed in this delta and verified in place at HEAD**
(#3053/#3054/#3055/#3056/#3057), and every regression guard this skill asks for
still holds: BA2 v2/v3 dispatch, the `geometries\` head, the LOD-slot sentinel
skip, `XCLL_SIZES_STARFIELD = [28, 108]`, the `base_layer` collider gate, the
#1567 LIGH-`DAT2` decode and the #1568 named PDCL skip.

### The delta lead, answered

The sweep's emphasis was the four Starfield water commits
(`148780cb` roughness, `4c81531f` oceanness, `f104ad69` ripple controls,
`900b028c` concentration controls, plus `fd4c2438` underwater absorption).
The **parser** half of that work is correct — independently byte-verified
against all 15 `Starfield.esm` WATR records. The **translation/shading** half
is not: on real data the two Starfield-specific optical controls both collapse.

| Control | Vanilla authored range (n=15) | What reaches the shader | Verdict |
|---|---|---|---|
| `absorption_ranges` (DNAM 4/8/12) | `0.1656 / 0.0962 / 0.0763` on 14 of 15 | used as a **divisor** → smallest channel attenuates most | **inverted + opaque at 0.18 wu** (SF-2026-08-20-D8-01, HIGH) |
| `concentration` (DNAM 16/20/24/28) | `0.019 … 20.0` | clamped to `0..1`; **41 of 60** values clamped away | density saturates to exactly `1.0` on **12 of 15** records (SF-2026-08-20-D8-02, MEDIUM) |
| `roughness` (DNAM 148) | `0.05 … 0.22` | `noise_falloff.z` + `sun_specular_power` | **lands correctly** |
| ripple / displacement (DNAM 72/76/80) | `0.975 / 1.0 / 0.05` | `displacement[0..2] ← [80, 72, 76]` | **lands correctly** |

This is reachable, not theoretical: `Starfield.esm` authors **6,550 `XCLW`**
water-height sub-records and **0 `XCWT`** anywhere, so exterior water resolves
through the worldspace `NAM2` fallback
(`cell_loader/exterior.rs:1145`, `cell.water_type_form.or(wctx.default_water_type_form)`)
into one of those same 15 WATR records — and the feature matrix lists Starfield
exterior grid / LAND / world streaming as ✓.

### The four dispatch leads

1. **Starfield `DNAM` = 152 B, FO76 `DNAM` = 148 B = Starfield minus the
   trailing roughness float.** Confirmed. My independent offset census of all
   15 records reproduces the `/audit-esm` table byte-for-byte, and the Starfield
   decoder's field map (`decode_dnam_starfield`, `water.rs:1161-1266`) matches it
   at every offset checked. **Dedup → `ESM-2026-08-20-D5-03`** for the FO76 half;
   not re-filed.
2. **Rain/displacement simulator alignment.** Starfield sits with FO4/FO76 —
   **correct**. DNAM 64/68/72/76/80 = `0.4 / 0.5 / 0.975 / 1.0 / 0.05` on 14 of
   15 records (the GECK displacement-simulator run, start size `0.05` on 15/15),
   and `decode_dnam_starfield:1218` zips `displacement ← [80, 72, 76]`, putting
   the displacement start size in slot 0 as the field doc requires.
   **Dedup → `ESM-2026-08-20-D5-04`** (the three misaligned arms); not re-filed.
3. **`texture_slot_layout` written on 1 of 4 shader-property branches.**
   Starfield population measured: **927 `BSEffectShaderProperty` blocks across
   742 NIFs (416 of them carry no `BSLightingShaderProperty` at all)**, and
   **0 `BSSkyShaderProperty` / 0 `BSWaterShaderProperty`** in the whole 89,276-NIF
   corpus. Runtime impact today is **zero** — see SF-2026-08-20-D8-03.
4. **The new Starfield real-data water assertion.** Genuinely load-bearing:
   `authored_absorption_records > 0` (`parse_real_esm.rs`) fails under
   `WaterParams::default()` (`absorption_ranges: [0.0; 3]`) and the 15 real
   records satisfy it (14 at `0.1656`, one at `0.30`). Its FO3 sibling gained no
   equivalent. It is **not offset-discriminating**, but that weakness is already
   filed as `ESM-2026-08-20-D8-01`; not re-filed.

### What this pass found

**Six new findings: 0 CRITICAL, 1 HIGH, 1 MEDIUM, 4 LOW.**

---

## Dimension Findings

| Dimension | New findings |
|---|---|
| 1 — BA2 v2/v3 LZ4 block decompression | 0 |
| 2 — BSGeometry mesh extraction | 0 |
| 3 — CDB material database correctness | 0 |
| 4 — Starfield ESM resolve-rate baseline | 0 (not re-measurable, see *Coverage Gaps*) |
| 5 — ESM + cell bring-up regression surface | **1** (LOW) |
| 6 — NIF shader blocks, BSVER 155+ | **1** (LOW) |
| 7 — Real-data validation | 0 |
| 8 — NIFAL / WATAL canonical translation | **3** (HIGH, MEDIUM, LOW) |
| 9 — BGSM/BGEM external material flow | **1** (LOW) |

---

### Dimension 1 — BA2 v2 / v3 LZ4 block decompression — 0 findings

Independently opened **all 129 Starfield archives** with an out-of-tree reader
built from `crates/bsa/src/ba2.rs`'s own header contract: **129/129 opened, 0
errors**.

```
v2 GNRL zlib -> 92     v2 DX10 zlib -> 22     v3 DX10 lz4 -> 15
```

The 15 v3 archives are exactly `Textures01..11`, `TexturesPatch01/02`,
`LODTextures01/02` — byte-identical to the 2026-08-16 split, so the v3
12-byte-extension offset and the `compression_method` read at that offset are
still correct (a wrong offset would have mis-read `3` and either errored or
selected zlib). No v3 GNRL exists in vanilla, matching the module doc. The
version match at `crates/bsa/src/ba2.rs:226-273` remains exhaustive over
`{1,2,3,7,8}` with a hard `InvalidData` return on an unsupported method, and
per-chunk raw-vs-compressed selection is still `packed_size == 0` in both
`extract_general` and `extract_dx10`.

The only delta touch is `6b49dd83` (*Fix #2596: correct BTDX v8 mesh-only doc
drift*) — comment-only.

Open, not re-filed: #2360, #2097, #2584, #2585.

### Dimension 2 — BSGeometry mesh extraction — 0 findings

Corpus shape re-measured independently from the NIF headers:

```
BSGeometry blocks            407,027      NiIntegerExtraData     407,027
BSSkin::Instance              19,633      BSSkin::BoneData        19,633
BSWeakReferenceNode           15,104      BSClothExtraData         1,040
```

`Starfield - Meshes01.ba2` alone carries **288,231 `.mesh` companions, 100% of
them under a bare `geometries\<hash>\<hash>.mesh` head** — so #1292's invariant
(`archive.rs:103`, `head.eq_ignore_ascii_case(b"geometries\\")` passes the path
through untouched) is still the one that matters, and it is still in place with
its `normalize_mesh_path` guards. The #1828/#1829 sentinel skip is still on both
arms (`bs_geometry.rs:47` Stage A `find_map`, `:108` Stage B loop), each testing
`!vertices.is_empty() && !triangles.is_empty()`.

Open, not re-filed: #2361, #2362.

### Dimension 3 — CDB material database correctness — 0 findings

Both 2026-08-16 findings are fixed and verified at HEAD:

- **#3054 / SF-D3-01** — `sf_cdb_cache` is now
  `Mutex<HashMap<String, Option<CdbHeaderInfo>>>` (`material.rs:147`), i.e. it
  caches the *probe result*, not the 233 MB of inflated bytes. Exactly the
  suggested fix.
- **#3055 / SF-D3-02** — `ComponentDatabaseFile::parse_with_limits` +
  `ParseLimits` landed (`reader.rs:31-58`, `:152-162`) and rejects the object
  tree *before* materialising it, with `Error::ParseBudgetExceeded`. `parse()`
  still defaults to `ParseLimits::unlimited()`, which is the documented,
  deliberate shape ("callers loading untrusted or memory-constrained content
  should choose a finite limit") — not re-filed.

`discover_starfield_cdbs` (`material.rs:167`) still scans `list_files()` for
every matching CDB rather than extracting a hardcoded base path (#1571), and
`register_starfield_cdb` still orders `peek_magic` → `probe_header`.
`323f0556` added the #2359 Phase-2 deferral invariant test.

Open, not re-filed: #2359.

### Dimension 4 — Starfield ESM resolve-rate baseline — 0 findings

`--sf-smoke citycydoniamainlevel` requires building `byroredux`, which the
briefing forbids. The 2026-08-16 figure (**25,437 / 27,898 = 91.2%**) is carried
forward unverified. The two structural guards it depends on were checked
statically instead and both hold: the LIGH `DAT2` decode
(`crates/plugin/src/esm/cell/support.rs:192`, #1567) and the named PDCL skip
(`crates/plugin/src/esm/records/mod.rs:377-383`, which still pushes
`*b"PDCL"` into `index.skipped_unconsumed_groups` before the one-shot warn,
#1568). `.claude/audit-baselines/sf-esm/*.tsv` were read, not regenerated.

The one delta change that *could* move the number is `6f5bf1fe` (#3056), and it
moves it in the right direction — see Dimension 5.

Open, not re-filed: #2637.

### Dimension 5 — ESM + cell bring-up regression surface — 1 finding

Verified clean at HEAD: `XCLL_SIZES_STARFIELD: &[usize] = &[28, 108]`
(`walkers.rs:57`, selected for `GameKind::Starfield` at `:95`);
HEDR-0.96 → `GameKind::Starfield` (`reader.rs:125,153`); the #1294 `base_layer`
gate (`spawn/mesh_instance.rs:907`); the #1284 `SkinSlotPool` cap-sizing;
`XCWT`/`XCLW` parsing in both the interior (`walkers.rs:221,239`) and exterior
(`wrld.rs:367,375`) walkers.

**#3056 / SF-D5-01 fixed and verified**: `build_static_object_from_subs` now has
a `b"MODL" if is_armor && sub.data.len() == 4` arm (`support.rs:67-71`) that
consumes the fixed-width ARMA FormID silently, and `parse_armo` resolves it via
`EsmIndex::armor_addons`. The 1,480 spurious `#1620 corrupt MODL` WARNs per
Starfield ESM parse are gone.

**#3057 / SF-D8-01 fixed and verified**: `slot_role.rs:17-24` now carries an
explicit *"Starfield / FO76 scope"* block recording that a zero Starfield hit in
that table is a format boundary, not an unmeasured gap.

#### SF-2026-08-20-D5-01: Starfield's `DNAM[0]` maps to `fog_far`, producing a 3.45–20 world-unit above-water ramp where every other game authors 58–4,710
- **Severity**: LOW
- **Dimension**: 5 — ESM + cell bring-up regression surface
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:1163-1166`
  (`decode_dnam_starfield`), consumer `byroredux/src/env_translate.rs:557`,
  GPU sink `crates/renderer/src/vulkan/water.rs:81` (`deep.a`)
- **Status**: NEW
- **Description**: `decode_dnam_starfield` opens with
  `p.fog_near = 0.0; p.fog_far = depth.max(1.0)` reading DNAM offset 0. On all
  15 vanilla records that field is `3.45 … 20.0`. `WaterParams::default()` is
  `fog_near 80.0 / fog_far 600.0`, and the same field measures `86.0` on 39 of
  47 FO76 records, `58` on FNV's `NVCleanWaterGS` and `110 … 4,710` on Skyrim —
  a 5–75× scale difference for a field the parser treats as the same quantity.
- **Evidence**: measured over `Starfield.esm`, DNAM offset 0, n = 15:
  ```
  8.0 ×2   4.0 ×2   12.0 ×2   3.45   5.0   10.0   12.5   7.0   3.5   5.17   20.0   9.0
  ```
  `deep.a` is also the refraction-**miss** distance handed to
  `absorbWaterColumn` (`water.frag:962-965`), so the same value doubles as a
  ray-length fallback.
- **Impact**: the shared `t = clamp((hitDist - fogNear) / span)` ramp
  (`water.frag:468`) saturates within 8 world units on Starfield instead of
  hundreds, so the legacy scalar absorption term is pinned at its far end for
  essentially every fragment. This compounds SF-2026-08-20-D8-01 but is
  independent of it.
- **Premise caveat**: I could **not** establish what DNAM 0 actually means on
  Starfield. The `/audit-esm` census labels it *"depth amount"*, which is not
  necessarily a distance in the same units as Skyrim's fog range. Reporting the
  divergence, not a corrected semantics — per the no-guessing rule this needs a
  byte-level source (xEdit SF1 `wbStruct(DNAM)` or a shipped water screenshot
  comparison), not a chosen constant.
- **Related**: SF-2026-08-20-D8-01, SF-2026-08-20-D8-02, `ESM-2026-08-20-D5-03`.
- **Suggested Fix**: decide the field's semantics from a source before changing
  anything. If it is a *depth scale* rather than a fog distance, it needs its own
  `WaterParams` slot and `fog_near`/`fog_far` should stay at their defaults for
  Starfield; if it is a distance, record the unit in the field doc so the 75×
  spread stops looking like a bug.

### Dimension 6 — NIF shader blocks, BSVER 155+ — 1 finding

The delta's highest-risk Starfield change is `6d7df853` (**#2622**), which moved
8 bytes: `read_wetness_block`'s `metalness`/`unknown_1` are now gated
`bsver < STARFIELD` (`shader.rs:1344-1348`), and the `BSSPLuminanceParams` quad
is read unconditionally on FO76 **and** Starfield. The commit's corpus evidence
came from `Starfield - Meshes01.ba2` only.

**I independently verified the +8-byte shift on the bsver the commit did not
sample.** Using the NIF header's block-size array to bound each block exactly,
and searching for the documented luminance quad
`(100.0, 13.5, 2.0, 3.0)` = `00 00 C8 42 | 00 00 58 41 | 00 00 00 40 | 00 00 40 40`:

| Archive | bsver | BSLSP blocks scanned | quad found | bytes after quad → block end |
|---|---:|---:|---:|---|
| `Meshes01.ba2` | 173 | 14,561 | 313 | **30 on 313/313** |
| `MeshesPatch.ba2` | 175 | 17,204 | 256 | **30 on 256/256** |
| `FaceMeshes.ba2` | 175 | 13,713 | 0 | (all material-reference stubs) |

So the 38 → **30**-byte opaque `starfield_tail` holds on **bsver 175 as well as
173**, and the quad really does sit immediately before it. #1606's capture-to-
`block_size` contract is intact. The candidate finding *"#2622 is validated on
bsver 173 only and 41% of the BSLSP population is 175"* is therefore **disproved
on data** — recorded below so it is not re-chased.

Corpus bsver distribution, for the record (89,276 NIFs, **0 header parse
errors**): `173 → 57,826 · 175 → 30,799 · 172 → 638 · 174 → 13`.
`BSLightingShaderProperty` blocks: 406,100 — byte-identical to the 2026-08-16
count. `Starfield - Meshes02.ba2` (7,552 NIFs) carries **zero** BSLSP blocks.

Open, not re-filed: #2624, #2639. (#2622 is CLOSED and its fix verified above.)

#### SF-2026-08-20-D6-01: `water.vert`'s `WaterParams` still documents `noise_falloff.yzw` as reserved after `148780cb` put Starfield roughness in `.z`
- **Severity**: LOW
- **Dimension**: 6 — GPU struct/shader lockstep (Starfield slice)
- **Location**: `crates/renderer/shaders/water.vert:107`, against
  `crates/renderer/shaders/water.frag:95-97` and
  `crates/renderer/src/vulkan/water.rs:107-109`
- **Status**: NEW
- **Description**: the three copies of the `WaterParams` std140 record agree on
  *layout* (22 vec4 slots, verified field-by-field; the
  `size_of::<GpuWaterParams>() == 352` pin at `water.rs:151` still holds and both
  `.spv` blobs were recompiled in the same commit as their sources, `1a428278`).
  They disagree on *contract documentation*: `water.vert:107` still reads
  `// x = authored Skyrim noise-falloff distance; yzw reserved.` while `.y` is
  the Blend-Normals gate, `.z` is the Starfield surface roughness (added by
  `148780cb`) and `.w` is Skyrim's Specular Radius.
- **Evidence**: `water.frag:95-96` — *"y = Blend Normals gate; z = Starfield
  surface roughness; w = Skyrim Specular Radius"*; producer
  `byroredux/src/render/water.rs:287-292` fills all four. A second, cross-game
  instance of the same drift sits at `crates/renderer/src/vulkan/water.rs:136-137`
  (`uv_offset`: *"zw are reserved for future transform terms. Cell WATR surfaces
  upload zero."*) while both shaders and `render/water.rs:335-339` define
  `.z` = flow-map bindless index bit-cast and `.w` = authored flow-map scale, and
  cell WATR uploads `u32::MAX` + the authored scale — not zero.
- **Impact**: no runtime effect. It matters because
  `feedback_shader_struct_sync` makes these three copies a lockstep contract:
  the next person to need a free slot reads `water.vert` and takes `.z`.
- **Related**: #2763 (the same class on `water.vert`'s `GpuInstance` comment),
  `feedback_shader_struct_sync`.
- **Suggested Fix**: copy `water.frag:95-96`'s wording into `water.vert:107`, and
  correct the `uv_offset` doc in `vulkan/water.rs` to match its own uploader.

### Dimension 7 — Real-data validation — 0 findings

129/129 archives open. 89,276/89,276 NIF **headers** parse with 0 errors, and the
block-type census matches the 2026-08-16 Rust-side figures exactly where the two
overlap (406,100 BSLSP; 407,027 BSGeometry; 15,104 `BSWeakReferenceNode`), which
is the strongest cross-check available without `cargo`. Full block-body parse
rate (`parse_rate_starfield_all_meshes`) and the #2105 NiUnknown residual of 6
are **carried**, not re-measured — see *Coverage Gaps*.

The `BSWeakReferenceNode` 2-byte gap fix (#2105, gated `bsver >= SF_FORM_ID` =
173) is still in place, and its gate is meaningful on this corpus: 638 NIFs sit
at bsver **172**, below the gate, and 30,812 at 174/175 above it.

### Dimension 8 — NIFAL / WATAL canonical translation for Starfield — 3 findings

`translate_material` is still the single `ImportedMesh → Material` boundary and
`Material::metalness`/`roughness` are still plain resolved `f32` — no per-draw
`Option<f32>` plumbing reintroduced. The three findings below are all on the
**WATAL** half (the `WatrRecord → WaterMaterial → GpuWaterParams` chain), which
is where this cycle's Starfield work landed.

#### SF-2026-08-20-D8-01: Starfield's per-channel absorption is applied as a divisor, inverting the hue and driving every vanilla Starfield water opaque within 0.18 world units
- **Severity**: HIGH
- **Dimension**: 8 — WATAL canonical translation
- **Location**: `crates/renderer/shaders/water.frag:493-500`
  (`absorbWaterColumn`) and `byroredux/src/systems/water.rs:496-518`
  (`underwater_color_at_depth`); parser source
  `crates/plugin/src/esm/records/misc/water.rs:1172-1180`; contract text
  `crates/core/src/ecs/components/water.rs:240-243` and `docs/engine/watal.md:321-322`
- **Status**: NEW
- **Description**: both consumers treat `absorption_ranges` as a **1/e
  distance** and divide by it:
  ```glsl
  // water.frag:495-498
  channelTransmission = exp(
      -hitDist * (1.0 + concentrationDensity)
      / max(authoredRanges, vec3(0.01))
  );
  ```
  ```rust
  // systems/water.rs:512
  (color[index] * (-optical_depth / range).exp()).clamp(0.0, 1.0)
  ```
  The vanilla data is ordered like an **extinction coefficient**, not a
  distance. Dividing therefore attenuates the *smallest*-valued channel the most
  — blue — so the surviving radiance is red, and the magnitudes make the whole
  column opaque almost immediately.
- **Evidence**: all 15 `Starfield.esm` WATR records, DNAM 4/8/12, walked with an
  independent ESM reader:
  ```
  R = 0.16558   G = 0.09624   B = 0.07627     on 14 of 15 records
  R = 0.30000   G = 0.07500   B = 0.01000     WaterSulfuric (the only variant)
  ```
  Two independent arguments, both unit-free:
  1. **Ordering.** `R > G > B` on every record. As an absorption *distance* this
     says red penetrates 2.17× further than blue — i.e. `WaterClear`,
     `WaterOceanClear`, `WaterClearLake` and `WaterClearNitrogen` all render
     red-transmitting. As a *coefficient* the same ordering is the textbook
     water curve (red absorbed first). Rescaling the unit cannot flip an
     ordering.
  2. **Magnitude.** Solving `channelTransmission < 0.01` for the current
     expression, with the real `concentrationDensity` each record produces:

     | record | opaque at, current `/range` | opaque at, `*coeff` |
     |---|---:|---:|
     | `WaterClear` / `WaterOceanClear` / 10 others | **0.176 wu** | 13.9 wu |
     | `WaterSulfuric` | **0.026 wu** | 8.8 wu |
     | `ENV_Test_CorrosivePuddle_OBSOLETE` | **0.236 wu** | 18.7 wu |

     0.18 world units is under 3 mm at Bethesda scale. The reciprocals of the
     authored triple — 6.0 / 10.4 / 13.1 wu — are commensurate with the same
     record's own DNAM-0 depth value (3.45–20), which is what a real range
     triple would look like.

  The underwater half is the same defect at a second site: at 1 wu of camera
  submersion `underwater_color_at_depth` returns `color * exp(-6.0)` ≈ `color *
  0.0025`, so the authored underwater tint is black on every Starfield water.
- **Impact**: every Starfield water surface renders as flat `deep_color` with no
  refraction and a red cast in the vanishing near band, and the underwater
  post-process tint is crushed to black. This is **reachable**: `Starfield.esm`
  authors 6,550 `XCLW` heights and 0 `XCWT`, so exterior cells take the
  worldspace `NAM2` fallback at `byroredux/src/cell_loader/exterior.rs:1145`
  into these same 15 records, and the feature matrix lists Starfield exterior
  grid / LAND / streaming as ✓. Other games are unaffected — the branch is gated
  on `any(greaterThan(authoredRanges, 0))` and only `decode_dnam_starfield`
  populates the field (FO76's decoder, per `ESM-2026-08-20-D5-03`, drops it).
- **Why the tests miss it**: `resolve_water_material_carries_starfield_absorption_ranges`
  (`env_translate.rs:2490-2510`) uses `absorption_ranges: [12.0, 34.0, 56.0]` —
  a synthetic triple 100–700× the real values *and in the opposite order*.
  `starfield_absorption_attenuates_channels_independently_after_near_plane`
  (`systems/water.rs`) uses `[10.0, 20.0, 40.0]`. Both fixtures encode the
  distance hypothesis, so neither can falsify it. The only real-data guard,
  `installed_masters_water_fields_are_finite_and_ordered`, asserts finiteness and
  `far >= near`, both of which survive.
- **Related**: SF-2026-08-20-D5-01 (compounds it), SF-2026-08-20-D8-02 (the
  `(1.0 + concentrationDensity)` factor that doubles the rate),
  `ESM-2026-08-20-D5-03` (FO76 never reaches this path),
  `ESM-2026-08-20-D8-01` (the blind real-data guard), #2785.
- **Suggested Fix**: establish the field's authored unit from a source before
  editing — do not pick a constant. If the values are extinction coefficients,
  both sites become `exp(-distance * coeff)` and the field should be renamed
  off `_ranges` in `WaterParams`, `WaterMaterial`, `GpuWaterParams`, both shaders
  and `watal.md`. Whatever the outcome, replace the two synthetic fixtures with
  a `#[ignore]` real-data assertion pinned to a named `Starfield.esm` record —
  e.g. `WaterOceanClear` must stay meaningfully transmissive at 5 world units.

#### SF-2026-08-20-D8-02: `env_translate` clamps Starfield water concentrations to `0..1`, but vanilla authors `0.019 … 20.0` — 41 of 60 values are clamped away and the shader's density term saturates on 12 of 15 records
- **Severity**: MEDIUM
- **Dimension**: 8 — WATAL canonical translation
- **Location**: `byroredux/src/env_translate.rs:865-869`; consumer
  `crates/renderer/shaders/water.frag:477-487`; parser source
  `crates/plugin/src/esm/records/misc/water.rs:1181-1188`
- **Status**: NEW
- **Description**: `900b028c` carried the Starfield concentration block through
  to the GPU, and `4c81531f` made its fourth component (oceanness) feed both
  the absorption density and the forward-scattering term. Translation clamps the
  whole quad to `0..1`:
  ```rust
  for (dst, src) in mat.concentration.iter_mut().zip(rec.params.concentration) {
      if src.is_finite() && src > 0.0 { *dst = src.clamp(0.0, 1.0); }
  }
  ```
  The authored values are not in `0..1`. The clamp turns a per-water
  discriminator into a constant.
- **Evidence**: all 15 `Starfield.esm` records, DNAM 16/20/24/28:
  ```
  WaterClear        8.840   6.594   4.710  | 0.514      WaterMudBrown   7.392  15.580  18.550 | 1.000
  WaterSulfuric     0.000  19.348  20.000  | 0.000      WaterSilt       2.682  20.000   2.898 | 0.000
  WaterOceanAlgae  19.420   7.608  11.232  | 1.000      WaterAlgae     17.608  15.652  16.812 | 0.000
  ENV_Test…         0.059   0.436   0.019  | 1.630
  ```
  **41 of 60** authored values exceed `1.0`. Feeding the clamped result through
  the shader's own expression
  (`clamp(dot(conc.rgb, vec3(0.25,0.50,0.25)) + conc.a*0.25, 0, 1)`) gives
  `concentrationDensity == 1.0` **exactly** on **12 of 15** records; the
  remaining three are `0.750` (`WaterSulfuric`, one channel authored 0),
  `0.925` (`WaterClearSterile`) and `0.487` (the OBSOLETE test record). So
  `WaterClear` and `WaterMudBrown` — the two records whose concentrations are
  most obviously meant to differ — produce a bit-identical density.
  Oceanness's absorption contribution is fully masked as a side effect: the RGB
  dot term already reaches `1.0` before `conc.a * 0.25` is added, so the
  `4c81531f` oceanness-into-absorption path is dead on 12 of 15 records.
  (Its *other* use, `oceanScatter` at `water.frag:1061`, reads
  `push.concentration.a` directly and does survive — 0.0 / 0.514 / 0.699 /
  1.0 across the set.)
- **Impact**: the "carry Starfield concentration controls" feature reaches the
  GPU but conveys no information for vanilla content; every ocean, lake, puddle,
  algae pool and mud pool gets the same optical density. Bounded and
  non-crashing, hence MEDIUM rather than HIGH — but it is also the `(1.0 + d)`
  factor that doubles SF-2026-08-20-D8-01's attenuation rate on 12 of 15
  records.
- **Why the tests miss it**: `resolve_water_material_carries_starfield_absorption_ranges`
  (`env_translate.rs:2496`) uses `concentration: [0.2, 0.4, 0.6, 0.8]` — every
  component chosen inside the clamp window, so `assert_eq!` passes and the clamp
  is never exercised.
- **Related**: SF-2026-08-20-D8-01, `ESM-2026-08-20-D8-01`, #2888.
- **Suggested Fix**: do not clamp at the translation boundary. Either carry the
  authored magnitude and normalise inside the shader against a documented
  reference concentration, or normalise at the boundary by a source-derived
  maximum — and pin it with a real-data test asserting that `WaterClear` and
  `WaterMudBrown` produce *different* `concentration` vectors.

#### SF-2026-08-20-D8-03: `texture_slot_layout` is written on 1 of the 4 dedicated-shader branches; the Starfield population that defaults to `Skyrim` is 927 blocks / 416 whole meshes, with zero consumers today
- **Severity**: LOW
- **Dimension**: 8 — NIFAL canonical translation
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs:105`
  (the only writer), against the three sibling branches dispatched at `:70-74`;
  field at `crates/nif/src/import/types.rs:558`; sole consumer
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:116`
- **Status**: NEW — **cross-audit**: the generic defect belongs to the NIF /
  NIFAL sweep; this entry exists only to attach the Starfield measurement so it
  is not re-derived
- **Description**: `apply_dedicated_shader_property` calls four branches;
  only `apply_bs_lighting_shader` assigns
  `info.texture_slot_layout = TextureSlotLayout::from_bsver(scene.bsver)`. A mesh
  whose material comes from a `BSEffect` / `BSSky` / `BSWater` property keeps
  `TextureSlotLayout::default()` = `Skyrim` regardless of `bsver`. (The assignment
  is correctly placed **before** the `material_reference` early return at `:131`,
  so Starfield's 99.4% stub majority is unaffected.)
- **Evidence**: census over all 89,276 vanilla Starfield NIFs:
  ```
  BSLightingShaderProperty   406,100 blocks   (73,178 NIFs carry one)
  BSEffectShaderProperty         927 blocks   (742 NIFs; 416 carry NO BSLSP)
  BSSkyShaderProperty              0
  BSWaterShaderProperty            0
  ```
  The 416 effect-only NIFs are dominated by LOD marker meshes
  (`meshes\lod\generated\landscape\caves\template\markers\*_lod_[0-3].nif`).
  Runtime impact is **zero**: the only read of the field builds the
  `TextureSlotContext` used to route a **REFR texture override**, and the
  2026-08-16 pass measured `XATO = XTXR = XMSP = 0` across 1,971,135
  `Starfield.esm` REFRs — a count this pass corroborates from the other side
  (`XCWT` is likewise 0 in the whole 1.4 GB master, i.e. Starfield does not use
  the FormID-override sub-record family at all).
- **Impact**: latent only. It becomes live the moment either (a) a Starfield
  mod authors an XTXR, or (b) #2359 Phase 2 makes the CDB a role producer for a
  BSEffect-hosted material.
- **Related**: #2695, #3057 (the scope note added this cycle), #2359, #3071.
- **Suggested Fix**: hoist the `from_bsver` assignment into
  `apply_dedicated_shader_property` so it is unconditional, and record the
  Starfield populations above beside the `slot_role.rs:17-24` scope note.

### Dimension 9 — BGSM/BGEM external material flow — 1 finding

**#3053 / SF-D9-01 fixed and verified**: `92f29ad1` widened the CDB-PBR gate
from `.mat` to `.mat | .bgsm | .bgem` behind the same `has_starfield_cdb()`
capability check (`material.rs:1061-1063`), so the 1,639 orphaned Starfield
shader properties across 234 paths now take the Disney-BSDF routing their
`.mat` neighbours get, and a one-shot per-path `log::warn!` makes the missing
payload visible (`:1099-1114`). `merge_external_material`'s signature is still
narrowed to `&mut ImportedMaterial` — no NIFAL boundary widening. `e1fc24d6`
(#2601) added resolve-failure tracking at the merge site; `ee927ed5` (#2627)
wired `inner_layer_texture`; `b87544f0` (#2700) restored unconditional `is_pbr`
for BGSM resolves. None of those have vanilla-Starfield population (0 `.bgsm`
and 0 `.bgem` files across 129 archives).

#### SF-2026-08-20-D9-01: the #3053 fix makes the BGSM/BGEM resolver unreachable for any session with a Starfield CDB registered
- **Severity**: LOW
- **Dimension**: 9 — BGSM/BGEM external material flow
- **Location**: `byroredux/src/asset_provider/material.rs:1061-1063` and the
  `return MergeOutcome::PresenceOnly` at `:1115`
- **Status**: NEW
- **Description**: the widened gate is
  `let starfield_named_material = path.ends_with(".mat") || path.ends_with(".bgsm") || path.ends_with(".bgem"); if starfield_named_material && provider.has_starfield_cdb() { … return MergeOutcome::PresenceOnly; }`
  — it returns **before** the BGSM/BGEM dispatch further down. So once any
  Starfield CDB is registered on the provider, a `.bgsm`/`.bgem` path that
  *does* resolve to a real file is never parsed: `from_bgsm` stays false, the
  BGSM spec-glossiness translation is skipped, and every authored texture role,
  `glass_enabled` and PBR scalar in that file is discarded in favour of the
  presence-only `is_pbr = true` flip.
- **Evidence**: the early `return` at `:1115` precedes every `resolve_bgsm` /
  BGEM call site. The gate is a *provider* capability, not a per-path one, so it
  is not narrowed by which archive the mesh came from. Today this is vacuous for
  vanilla (0 `.bgsm`, 0 `.bgem` in 129 archives, which is exactly what motivated
  #3053), so it is a forward-looking narrowing rather than a live regression.
- **Impact**: a Starfield mod that ships genuine BGSM/BGEM sidecars — or any
  mixed session where a Starfield CDB is registered alongside FO4-era loose
  materials — silently loses all authored material data for those meshes, with
  the new one-shot warn describing it as *"has no external BGSM/BGEM payload"*
  even though one exists.
- **Related**: #3053 (the fix that introduced the shape), #2601, #2709.
- **Suggested Fix**: attempt `resolve_bgsm`/BGEM **first** for `.bgsm`/`.bgem`
  paths and fall through to the CDB-gated PBR flip only on a resolve miss. That
  keeps #3053's whole benefit (vanilla always misses) while restoring the
  resolver for the case where a payload is actually present.

---

## CRC32 Flag Table

No new empirical CRC32 → flag-name mapping was derivable, and the reason is
unchanged and now independently re-measured: **99.4% of Starfield's 406,100
`BSLightingShaderProperty` blocks are material-reference stubs**, whose
`sf1_crcs`/`sf2_crcs` arrays are empty by construction
(`crates/nif/src/blocks/shader.rs:798,805-806`), and the corpus contains only **927**
`BSEffectShaderProperty` blocks — the other CRC-array producer — spread over 742
NIFs. Starfield's flag vocabulary lives in the CDB, not the NIF. The table in
`crates/nif/src/shader_flags.rs` is unchanged; its `MODELSPACENORMALS` entry
remains correct but unexercised on this game (0 of 2,538 full-body blocks carry
it, per 2026-08-16).

## Remaining-Work Chain

Per `docs/engine/starfield-esm-roadmap.md` (Phases 0+1 done, 2–4 invalidated by
the 99.9%-parity measurement), in order:

1. **Starfield water optics** — new head of the list this cycle. The parser is
   correct and byte-verified; the translation/shading half needs
   SF-2026-08-20-D8-01 and -D8-02 resolved, and SF-2026-08-20-D5-01's semantics
   established, before Starfield exterior water is worth benching.
2. **Per-field CDB extraction** (#2359 Phase 2) — `parse_with_limits` +
   `ParseLimits` (#3055) removed the memory blocker; the reader still exposes no
   path→instance index. #3053's fix means the 234 orphaned `.bgsm` paths now
   share the `.mat` arm, so one lookup serves both.
3. **Exterior worldspace tiles.**
4. **Space-cell / planet / GBFM records** — `Starfield.esm` top-level GRUPs
   confirm the scale of what is still stubbed: `SFTR` 90.6 MB, `GBFM` 36.1 MB,
   `PNDT` 25.8 MB, `STDT` 12.0 MB, `BIOM` 5.3 MB.
5. **The #746/#747 NIF truncation tail** — not re-measurable this pass; carried
   at the 2026-08-16 figure (0 truncations, 6 `BSWeakReferenceNode` recoveries).

## Disproved Candidates

Recorded so they are not re-chased:

1. **#2622's +8-byte luminance shift is validated on bsver 173 only, and 41% of
   the vanilla BSLSP population is bsver 175.** The premise about the commit's
   corpus is true, but the conclusion is wrong: the luminance quad sits exactly
   30 bytes before block end on **256/256** sampled bsver-175 full-body blocks
   (`MeshesPatch.ba2`) as well as **313/313** bsver-173 blocks (`Meshes01.ba2`).
2. **The `.bgsm`→CDB widening (#3053) mis-routes FO4 content.** It cannot —
   `has_starfield_cdb()` is a provider capability and no FO4 session registers a
   CDB. (The *forward* narrowing it does cause is SF-2026-08-20-D9-01.)
3. **`parse()` is still an unbounded 9.19 GB entry point.** It defaults to
   `ParseLimits::unlimited()`, but that is the documented, deliberate contract
   #3055 shipped; the budget mechanism exists and rejects before materialising.
4. **`GpuWaterParams` drifted from the shader structs when the Starfield slots
   were added.** All three copies were walked field-by-field: 22 vec4 slots in
   the same order, the `== 352` static assert holds, and both `.spv` blobs were
   regenerated in the same commit as their `.frag`/`.vert` sources (`1a428278`).
   Only the *comments* drifted (SF-2026-08-20-D6-01).
5. **The new Starfield water assertion in `parse_real_esm.rs` is a tautology
   like its FO76 sibling.** It is not — `absorption_ranges` defaults to
   `[0.0; 3]`, so the assertion genuinely fails if the decoder stops reading
   DNAM 4/8/12. It is merely *weak* (any positive float in those slots passes),
   which is already `ESM-2026-08-20-D8-01`.
6. **`Starfield - Meshes02.ba2` carrying zero `BSLightingShaderProperty` blocks
   indicates a dropped archive.** It is genuine authoring: 7,552 NIFs at
   bsver 172/173, all shader-property-free (collision/marker/skeleton content),
   and its 62,685 `.mesh` companions resolve through the shared `geometries\`
   hash tree.

## Coverage Gaps

Three checks this skill mandates could not be executed under the suite briefing's
no-`cargo` rule, and are **carried from 2026-08-16 rather than re-verified**:

- **Dimension 4** — `--sf-smoke citycydoniamainlevel` resolve rate (91.2%,
  25,437/27,898). Its two structural guards were checked statically instead.
- **Dimension 7** — `parse_rate_starfield_all_meshes` block-**body** parse rate
  and the #2105 NiUnknown residual of 6. Header-level parsing (89,276/89,276, 0
  errors) and the block-type census were verified independently and match.
- **Dimension 3** — the CDB byte inventory (13 CDBs, 233 MB) was not re-run; the
  cache-shape fix that made it irrelevant was verified by reading instead.

Per `_audit-common.md`'s un-owned-subsystem list, this audit did not touch the
P2 gameplay slice, FaceGen, the mod runtime, FSR3, the Havok packfile reader, or
the debug server — none are Starfield-specific.

## Deduplication

Baseline: `/tmp/audit/issues.json` (400 issues, #2671–#3103), plus
`docs/audits/AUDIT_STARFIELD_2026-08-16.md`, `AUDIT_ESM_2026-08-20.md` and
`AUDIT_ESM_2026-08-16.md`. Older issue numbers are carried on the prior report's
word, as the briefing directs.

**CLOSED and verified fixed at HEAD** (not re-filed, not reported as
regressions): #3053, #3054, #3055, #3056, #3057, #2622, #2621, #2626, #2627,
#2700, #2601, #2596, #2359.

**OPEN, matched and deliberately not re-filed**: #2359, #2360, #2361, #2362,
#2097, #2584, #2585, #2624, #2637, #2639, #2763, #2787, #2882, #2888, #3071.

**Cross-audit dedup** (findings owned by a sibling audit in this same sweep,
referenced rather than duplicated): `ESM-2026-08-20-D5-03` (FO76 uses the
Starfield DNAM layout), `ESM-2026-08-20-D5-04` (rain/displacement start-size
swap in three arms), `ESM-2026-08-20-D8-01` (the real-data WATR guard cannot
detect an offset-map error).

TALLY: CRITICAL=0 HIGH=1 MEDIUM=1 LOW=4
