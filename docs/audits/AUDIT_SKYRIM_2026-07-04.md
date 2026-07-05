# Skyrim SE Compatibility Audit — 2026-07-04

**Scope:** ByroRedux readiness + regression coverage for *The Elder Scrolls V:
Skyrim Special Edition* content. Skyrim SE is the engine's renderer **control
bench** (Whiterun BanneredMare, 6 equipped NPCs) — both loose-mesh and cell
rendering already work, so this pass is **regression coverage** plus the
genuinely Skyrim-specific geometry / shader / equip / load-order risk surface.

**Method:** 7 dimension agents, each verifying live code by symbol-grep (not line
numbers) against nif.xml (`/mnt/data/src/reference/nifxml/nif.xml`) and the real
on-disk archives at `…/Skyrim Special Edition/Data/`. Engine not launched.
Bench-of-record: **Whiterun BanneredMare 3216 entities @ 362.8 FPS / 2.76 ms /
fence 0.98** (R6a-stale-14, `1c26bc25`, 2026-06-03). Skyrim NIF parse rate 100%
clean (18862 meshes). BSA v105 LZ4.

---

## Executive Summary

**3 findings, all LOW. No CRITICAL / HIGH / MEDIUM. 4 of 7 dimensions fully
clean.** This is the expected profile for a shipped control-bench title — the
value of the pass is the ~40 regression guards catalogued across the dimension
reports, not new bugs.

| Dimension | Verdict | Findings |
|-----------|---------|----------|
| 1 · BSTriShape packed geometry + SSE reconstruction | Effectively clean | SKY-D1-001 (LOW) |
| 2 · BSLightingShader / BSEffectShader type dispatch | Effectively clean | SKY-D2-001 (LOW) |
| 3 · NPC equip + FaceGen (M41) | Effectively clean | SKY-D3-001 (LOW) |
| 4 · Multi-master load order + TES5 cell-load | **CLEAN** | — |
| 5 · BSA v105 (LZ4) | **CLEAN** | — |
| 6 · Specialty blocks + real-data rendering | **CLEAN** | — |
| 7 · NIFAL canonical material translation | **CLEAN** | — |

All three findings are latent / doc-level: none affects any shipping Skyrim
content today. Each is a **maintenance trap** — a comment or constant that would
mislead a future edit into *breaking* a currently-correct path. That framing sets
their severity (LOW) and their real payoff (cheap now, expensive if they metastasize).

**Cross-dimension notes (not findings):**
- The audit checklist prose says BSA v105 decodes "LZ4 **block** via `lz4_flex::block`" —
  that wording is wrong; Skyrim SE v105 is LZ4 **frame** (`lz4_flex::frame::FrameDecoder`),
  and the code correctly uses frame. Block is the *Starfield BA2* path. Pinned by a
  frame≠block guard test. (Dim 5 corrected the checklist, not the code.)
- The checklist names `BsTriShape::parse_meshlod` for BSMeshLODTriShape; the live
  method is `BsTriShape::parse_lod(...).with_kind(MeshLOD)`. Routing intent is exactly
  correct — only the method name differs. (Dim 6 doc-precision note, no code change.)

---

## Findings

### SKY-D1-001: SSE global-buffer decoder gates the tangent quad on `VF_TANGENTS` alone, diverging from nif.xml's `(ARG & 0x18) == 0x18`
- **Severity**: LOW
- **Dimension**: BSTriShape Packed Geometry
- **Location**: `crates/nif/src/import/mesh/sse_recon.rs::decode_sse_packed_buffer` (the `has_tangents` binding + its use at the `Tangent` / `bitangent_z` read)
- **Status**: Regression of #1559 (the change introduced this divergence while claiming to remove one)
- **Description**: nif.xml `BSVertexDataSSE` has two distinct tangent-related predicates —
  `Bitangent X` gated on `(ARG & 0x11) == 0x11` (VF_VERTEX && VF_TANGENTS), and the
  `Tangent` + `Bitangent Z` quad gated on `(ARG & 0x18) == 0x18` (**VF_NORMALS && VF_TANGENTS**).
  The inline decoder `decode_bs_vertex_stream` (`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`)
  models both correctly. The SSE global-buffer decoder collapses both onto one boolean
  `has_tangents = vertex_attrs & VF_TANGENTS != 0` and reuses it for the tangent quad —
  correct for `bitangent_x`, wrong (missing the `&& VF_NORMALS` / 0x18 term) for the quad.
- **Evidence**: SSE decoder single gate:
  ```rust
  let has_tangents = vertex_attrs & VF_TANGENTS != 0;
  if has_tangents { tangent_xyz = Some([...]); bitangent_z = Some(...); off += 4; }
  ```
  Inline decoder two-predicate model:
  ```rust
  if vertex_attrs & VF_TANGENTS != 0 && vertex_attrs & VF_NORMALS != 0 { /* 0x18 tangent quad */ }
  ```
  nif.xml: `Tangent`/`Bitangent Z` both `cond="(#ARG# #BITAND# 0x18) == 0x18"`.
- **Impact**: **No live impact** — a descriptor with VF_TANGENTS set and VF_NORMALS clear is
  nonsensical (tangent space needs a normal) and does not occur in shipped Skyrim SE content,
  so both gates evaluate identically for every real body (this is why the SSE-recon chrome/magenta
  path is not regressed). The real risk is a **maintenance trap**: the #1559 comment asserts the
  inline decoder "gates on VF_TANGENTS alone" (it does not — it gates the quad on 0x18); a future
  maintainer trusting that comment could "align" the correct inline path to the wrong SSE gate and
  break the path that parses all 18862 Skyrim meshes at 100%.
- **Related**: #1559 (introduced), #796 (SSE tangent reconstruction), #795 (inline tangent convention)
- **Suggested Fix**: In `decode_sse_packed_buffer`, split the gate — keep `bitangent_x` on
  `VF_TANGENTS`, add `has_tangent_quad = has_tangents && vertex_attrs & VF_NORMALS != 0` for the
  `Tangent`/`bitangent_z` read, and correct the comment to cite the 0x18 predicate. Output is
  unchanged for all real content; this only fixes the byte-alignment gate.

### SKY-D2-001: `skyrim_slsf2::CLOUD_LOD` constant is off by one bit (its value is Anisotropic_Lighting)
- **Severity**: LOW
- **Dimension**: Shader-Type Dispatch
- **Location**: `crates/nif/src/shader_flags.rs::skyrim_slsf2::CLOUD_LOD`
- **Status**: NEW
- **Description**: `CLOUD_LOD` is defined `0x0020_0000` (bit 21) and documented "Bit 21 —
  `Cloud_LOD` on Skyrim". nif.xml `SkyrimShaderPropertyFlags2` places **Cloud_LOD at bit 20**
  (`0x0010_0000`) and **Anisotropic_Lighting at bit 21** (`0x0020_0000`). The constant's value
  is therefore Anisotropic_Lighting, and the doc-comment is wrong about which flag lives at bit 21.
  The sibling `fo4_slsf2::ANISOTROPIC_LIGHTING` already documents `0x0020_0000` correctly — the two
  modules disagree on the same numeric value.
- **Evidence**:
  ```rust
  /// Bit 21 — `Cloud_LOD` on Skyrim (NOT `Alpha_Decal` …).
  pub const CLOUD_LOD: u32 = 0x0020_0000;      // shader_flags.rs
  ```
  ```
  <option bit="20" name="Cloud_LOD"></option>            <!-- nif.xml SkyrimShaderPropertyFlags2 -->
  <option bit="21" name="Anisotropic_Lighting">Hair only?</option>
  ```
- **Impact**: None functionally today — the constant participates in no live decode (the live
  decal/two-sided helpers don't test it). Risk is latent: future code reaching for "Skyrim Cloud_LOD"
  via this constant would read Anisotropic_Lighting. The `walker.rs` comment ("flags2 bit 21 is
  `Cloud_LOD` on Skyrim") inherits the same off-by-one. **Note:** the value is currently pinned by
  `shader_flags.rs::tests` (`assert_eq!(skyrim_slsf2::CLOUD_LOD, 0x0020_0000)` and
  `assert_eq!(fo3nv_f2::ALPHA_DECAL, skyrim_slsf2::CLOUD_LOD)`) — any value change must update those.
- **Related**: nif.xml `SkyrimShaderPropertyFlags2`; correct sibling `fo4_slsf2::ANISOTROPIC_LIGHTING`; #414 (modern-vs-legacy decal split)
- **Suggested Fix**: Either (a) rename the constant to `ANISOTROPIC_LIGHTING` and add a separate
  `CLOUD_LOD = 0x0010_0000` (bit 20) if a Skyrim Cloud_LOD constant is wanted, or (b) if Cloud_LOD is
  the intended semantic, set the value to `0x0010_0000`. Update the doc-comment, the `walker.rs`
  comment, and the two pinning asserts in `shader_flags.rs::tests`. No behavioral change to shipping code.

### SKY-D3-001: `expand_leveled_form_id` docstring + inline comment claim multi-pick is unimplemented, but the code implements it
- **Severity**: LOW
- **Dimension**: NPC Equip + FaceGen
- **Location**: `crates/plugin/src/equip.rs::expand_leveled_form_id`
- **Status**: NEW
- **Description**: The doc comment states the "calculate for each item" flag (LVLF bit 1) is
  *unimplemented* and that multi-pick LVLIs "land all eligible entries", while describing single-pick
  as the only real behaviour. The code below branches on `lvli.flags & 0x02` and implements a genuine
  multi-pick vs single-highest-eligible split; LVLF flags are parsed and populated
  (`container.rs`, `record.flags = sub.data[0]`), so the branch is live. Doc rot that mis-describes the
  single-pick semantics (which pick the *highest* eligible, not "all").
- **Evidence**:
  ```rust
  /// … The "calculate for each item" flag (bit 1) is also unimplemented today   // docstring
  /// — multi-pick LVLIs land all eligible entries …
  let multi_pick = lvli.flags & 0x02 != 0;                                        // contradicts it
  if multi_pick { for entry in &eligible { expand_leveled_inner(entry.form_id, ...); } }
  else { let pick = eligible.iter().max_by_key(|e| e.level)...; expand_leveled_inner(pick.form_id, ...); }
  ```
  Test `expand_multi_pick_lands_all_eligible` (LVLI flag bit 1 = `0x02`) exercises the multi-pick branch.
- **Impact**: Documentation only — behaviour is correct. Risk is a future maintainer "adding" a
  multi-pick that already exists, or mis-reasoning about the single-pick default (highest-eligible, not
  all) during a leveled-gear audit.
- **Related**: M41 Phase 2 (#896)
- **Suggested Fix**: Update the docstring + inline comment to state multi-pick (LVLF bit `0x02`) is
  implemented and single-pick returns the highest-eligible entry (not "all eligible"). Keep the accurate
  `chance_none = 0` caveat.

---

## Shader-Type Coverage Matrix

`ShaderTypeData` trailing-field dispatch (`parse_shader_type_data`), verified field-for-field against
nif.xml `BSLightingShaderType` + the cond-gated `BSLightingShaderProperty` trailing fields. Parse =
correct byte count; Import = surfaced into `MaterialInfo`; Render = consumed by the material pipeline.

| Type | nif.xml name | ShaderTypeData arm | Trailing (Skyrim) | Parse | Import | Render |
|-----:|--------------|--------------------|-------------------|:-----:|:------:|:------:|
| 0 | Default | `None` | — | ✓ | ✓ | ✓ |
| 1 | Environment Map | `EnvironmentMap` | env_map_scale (f32) | ✓ | ✓ | ✓ |
| 2 | Glow | `None` (no GlowShader variant) | — | ✓ | ✓ | ✓ |
| 3 | Parallax | `None` | — | ✓ | ✓ | ✓ |
| 4 | Face Tint | `None` | — | ✓ | ✓ | ✓ |
| 5 | Skin Tint | `SkinTint` | Color3 | ✓ | ✓ | ✓ |
| 6 | Hair Tint | `HairTint` | Color3 | ✓ | ✓ | ✓ |
| 7 | Parallax Occ | `ParallaxOcc` | max_passes + scale (2×f32) | ✓ | ✓ | ✓ |
| 8–10 | Multi-decal / Height / Multi-index | `None` | — | ✓ | ✓ | ✓ |
| 11 | MultiLayer Parallax | `MultiLayerParallax` | thickness+refraction+inner-UV(2)+envmap (5×f32) | ✓ | ✓ | ✓ |
| 12–13 | Sparkle-stub / Noise | `None` | — | ✓ | ✓ | ✓ |
| 14 | Sparkle Snow | `SparkleSnow` | Vector4 | ✓ | ✓ | ✓ |
| 15 | Fog | `None` | — | ✓ | ✓ | ✓ |
| 16 | Eye Envmap | `EyeEnvmap` | cube scale + left(3) + right(3) (7×f32) | ✓ | ✓ | ✓ |
| 17–19 | Skin / Cloud / LOD-Land-Noise | `None` | — | ✓ | ✓ | ✓ |

- No arm over-reads (every no-trailing type consumes zero bytes) or under-reads (field counts exact).
- **FO76 `BSShaderType155`** (`parse_shader_type_data_fo76`) uses the *different* numbering
  (type 4 → `Fo76SkinTint` Color4, type 5 → `HairTint` Color3) and is BSVER-selected, so it never
  cross-contaminates the Skyrim `{5=SkinTint-Color3, 6=HairTint}` table.
- **`BSEffectShaderProperty`** field order/gating matches nif.xml for the Skyrim era; the FO4+
  env/normal textures and FO76 reflectance tail are correctly version-gated absent.
- **Disney/Burley lobe** (`MAT_FLAG_PBR_BSDF`) stays **provably unreachable** for vanilla Skyrim:
  `is_pbr` flips true only in the FO4+ BGSM/BGEM merge path, and vanilla Skyrim meshes resolve no
  external material file. Modded BGSM opting into PBR is the one legitimate path.

---

## Cell-Load Regression Status

TES5 cells parse through the unified `crates/plugin/src/esm/cell/` walker; compressed record groups
decompress; the multi-master path merges cross-plugin FormIDs. All eight Dim-4 checklist points hold
against live code, with passing guard tests this session (`esm::reader` 35/35, `byroredux load_order` 6/6).

| Guard | Location | Status |
|-------|----------|--------|
| Repeatable `--master` remap + missing/misordered loud-fail | `cell_loader/load_order.rs::build_remap_for_plugin`; `esm/reader.rs::FormIdRemap::remap` | code ✓ |
| Last-write-wins merge across record maps | `esm/records/index.rs::merge_from` (`HashMap::extend`) | code ✓ |
| Unresolved-REFR names the owning plugin | `cell_loader/references/mod.rs::plugin_for_form_id` + per-plugin breakdown | code ✓ |
| `.STRINGS` guard installed **per plugin** (#1553) | `cell_loader/load_order.rs::install_strings_guard` | tests PASS |
| ESL 0xFE light-space decode (#1554) | `esm/reader.rs::GlobalSlot::compose` | tests PASS |
| Deleted-REFR tombstone skip (#1660), doc in sync (#1781) | `esm/cell/walkers.rs::RECORD_FLAG_DELETED` (0x20); `esm/cell/mod.rs` doc | code ✓ |
| Real-`Skyrim.esm` cell walk finds `SolitudeWinkingSkeever` | `esm/cell/tests/integration.rs::parse_real_skyrim_esm` | code ✓ (data-gated) |
| TES5 compressed-record inflate | `esm/reader.rs` FLAG_COMPRESSED path (#990) | tests PASS |
| Min interior record set (CELL/REFR/STAT/LIGH/WEAP/ARMO/LAND/LTEX/TXST/ADDN) | `esm/records/`, `esm/cell/{support,walkers}.rs` | code ✓ |
| **Control-bench**: Whiterun entity count flat (real bhk collision) | Bench-of-record 3216 ent @ 362.8 FPS | no regression |

---

## Regression Guards Worth Pinning (top of the ~40 catalogued)

Highest-value first, across dimensions:

1. **#838 — BSLODTriShape → `NiLodTriShape`, NOT `BsTriShape`.** Three distinct trishape bodies
   (BSLODTriShape = NiTriBasedGeom; BSMeshLODTriShape = BSTriShape/MeshLOD; BSSubIndexTriShape =
   SubIndex) — never collapse them, or Skyrim tree-LOD byte-offset drift returns.
2. **SSE-recon chrome/magenta routing** — positions AND normals share the canonical `zup_to_yup_pos`;
   the on-disk bitangent triplet is routed as the Y-up tangent via the shared `bitangent_sign` helper.
   Drift here resurrects reconstructed-body chrome.
3. **#837 controller parsers** — `BsLagBoneController` (12 B) + `BsProceduralLightningController` (73 B)
   consume their exact tails; removing either reopens the per-sweep realignment-WARN burst that masks
   real drift.
4. **NIFAL single boundary** — `translate_material` has exactly two callers; `Material::classify_pbr`
   (per-draw) stays deleted; `resolve_pbr()` runs before `classify_glass_into_material`.
5. **#1873 chrome-flyer** — `specular_authored: self.has_material_data` wiring + the `!specular_authored`
   env-map dielectric return must both survive.
6. **BSA v105 = LZ4 frame, not block** — `lz4_flex::frame::FrameDecoder`; pinned by a frame≠block guard.
7. **Zero-based sibling auto-load** (`…0` → `…1..9`, guarded against `…10`) — feeds M35 distant-LOD
   textures from `Textures7`/`Meshes1`.
8. **`.btr` quad-local-normalized vs `.bto` world-absolute** — swapping either convention silently
   corrupts distant geometry.
9. **VWD full-model cull is forward scope (#1731), not a gap** — `is_visible_when_distant()` exists;
   consuming it to cull full models is deferred by design. Do not re-file.

---

## Recommended Next Step

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-07-04.md
```

All three findings are LOW doc/constant maintenance traps — cheap to fix, and each prevents a future
edit from breaking a currently-correct path. No blocking correctness issues on the Skyrim control bench.
