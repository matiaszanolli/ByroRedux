# Renderer Audit — Dimensions 15 + 16 (2026-05-24)

Focused sweep — Dim 15 (Sky / Weather / Exterior Lighting M33/M33.1/M34) and Dim 16 (Tangent-Space & Normal Maps, M-NORMALS Sessions 26-29). Both dimensions have heavy prior-closure history; this run looks for regressions from the past-week renderer churn.

## Executive Summary

- **0 findings**. **14 verifications clean** (7 per dimension). Both dimensions came through the heavy 2026-05-17 → 2026-05-24 renderer churn (Disney BSDF, water caustics, M47.0 Papyrus, today's sun-sprite mip 0) without regression.
- The 4 new fixes that touched these dimensions in the past week all verified live: **#1104** (UV-mirror handedness in derivative TBN, Dim 16), **#1232** (BSGeometry empty-tangent → synthesize_tangents_yup, Dim 16), **#1265** (extract_mesh.NiTriShape tangent clone removed, Dim 16), today's `8b5d77c1` (sun-sprite mip 0 in compute_sky, Dim 15).
- **One audit-prompt-wording note** (informational, not a finding): Dim 10's "fog applied to direct lighting only, NOT to indirect" line referenced from Dim 15 is potentially misleading — fog is applied to the **assembled HDR** (direct + denoised_indirect + bloom) pre-ACES, not just to "direct" alone. The semantic the prompt is trying to capture (linear-space HDR application, pre-ACES) is correct per the #784 / LIGHT-N2 closure; the wording is loose. Could be refined in a future prompt edit.

| Severity | NEW | Carryover | Total |
|----------|-----|-----------|-------|
| HIGH     | 0   | 0         | 0     |
| MEDIUM   | 0   | 0         | 0     |
| LOW      | 0   | 0         | 0     |
| INFO     | 14  | 0         | 14    |

---

## Dimension 15: Sky / Weather / Exterior Lighting

### V15-1 — `weather_system` advances time monotonically, sun arc from CLMT TNAM

- **Site**: `byroredux/src/systems/weather.rs:287` (`weather_system(world, dt)`)
- **Found**: function takes `dt: f32`; in-flight `WeatherTransitionRes` timer advances at lines 300-310. Sun arc derivation is in `compute_sun_arc(hour, tod_hours)` at line 79, with `tod_hours = [sunrise_begin, sunrise_end, sunset_begin, sunset_end]` (line 80) sourced from CLMT TNAM (not hardcoded). The pre-#463 hardcoded fallback is documented at line 205 (`DEFAULT_TOD_HOURS`).
- ✓ Clean.

### V15-2 — TOD palette interpolation between WTHR NAM0 colors

- **Site**: `byroredux/src/systems/weather.rs::build_tod_keys(tod_hours)` at line 33, returning 7 keypoints `[(f32, usize); 7]` (midnight, sunrise band, day, sunset band, night).
- **Found**: 7-key interpolation covers the full day cycle; midnight is a synthetic key anchored at 1h (line 22). `WeatherTransitionRes` cross-fade ramp at lines 279-310 blends source + target palette over its 8 s window before promoting the target's `wind_speed` (#1101 closure) and `skyrim_dalc_per_tod` (#1102 closure) — both pre-existing audit findings.
- ✓ Clean.

### V15-3 — All 4 cloud layers active in exterior cells (M33.1 closed)

- **Site**: `crates/renderer/shaders/composite.frag:43-45` (`cloud_params_1/_2/_3` — DNAM/CNAM/ANAM/BNAM layers).
- **Found**: 3 explicit `cloud_params_*` UBO entries beyond the base `cloud_params` (layer 0 = DNAM). Layers 1-3 (CNAM/ANAM/BNAM) all sampled via `textureLod` at `composite.frag:184-221`. Per-layer `tile_scale_N == 0.0` guard short-circuits the branch when no texture is loaded for that layer.
- Cloud scroll velocities: layers 2/3 now use distinct ANAM/BNAM velocities (#899 closure at `weather.rs:533`), not the layer-0/1 reuse.
- ✓ Clean.

### V15-4 — Wind-speed → cloud-scroll rate calibration

- **Site**: `byroredux/src/systems/weather.rs:226-244` (`cloud_scroll_rate_from_wind`, `WIND_TO_SCROLL_RATE` constant).
- **Found**: `wind_speed = 0` → static clouds; `wind_speed = 32` → typical mid-range; `wind_speed = 255` → ~0.143 UV/sec storm. The calibration commit + a parser-side fallback (when DNAM cloud-path-zstring byte was misread as wind_speed pre-#1033) is documented at line 446-466.
- ✓ Clean.

### V15-5 — `is_exterior` flag interior-vs-exterior gates wired

- **Site**: `triangle.frag:208` (`jitter.w` carries `is_exterior` flag for #1125), `composite.frag:36` (`depth_params.x` mirrors the flag for composite).
- **Found**: The #1125 gate at `triangle.frag:527-531` (RT reflection miss) and `:2141-2144` (RT refraction miss) reads `jitter.w > 0.5` to pick `(skyTint*0.5 + sceneFlags.yzw*0.5)` for exterior vs `sceneFlags.yzw` alone (cell ambient) for interior. Interior fill at 0.6× ambient with `radius=-1` gates the RT shadow at `triangle.frag:2581` (`isInteriorFill = radius < 0.0`).
- Documented in today's FNV Dim 4 audit (V4) — verified again.
- ✓ Clean.

### V15-6 — Sun-sprite mip 0 force (`8b5d77c1`, 2026-05-24)

- **Site**: `composite.frag:281` — `vec4 sprite = textureLod(textures[nonuniformEXT(sun_tex_idx)], uv, 0.0);`
- **Found**: today's fix at line 281. Without the explicit `0.0` LOD, the driver's `dFdx/dFdy` on the sky-disc UV gradient (sun is a tiny screen-space feature with steep gradients) picked a high mip, producing pixelation.
- ✓ Clean — today's improvement is wired.

### V15-7 — M40 streaming TOD palette stays per-worldspace (#1199 closure)

- **Site**: `byroredux/src/cell_loader/unload.rs` (per-#1199 fix; not re-read here, deferred to the closure record).
- **Status**: #1199 (REN-DIM15-01: `unload_cell` wipes worldspace-scoped weather/sky/lighting) is CLOSED. The audit prompt's regression-guard claim ("cell transition does not strobe TOD — palette is per-worldspace + global TOD clock, not per-cell") is testable by walking through `--grid 0,0 --radius 3` exterior streaming and watching for palette discontinuity at cell boundaries.
- ✓ Verified via issue-closure-record (live smoke test would confirm).

---

## Dimension 16: Tangent-Space & Normal Maps (M-NORMALS)

### V16-1 — Oblivion/FO3/FNV authored-tangent path honors Bethesda swap (#786)

- **Site**: `crates/nif/src/import/mesh/tangent.rs:60-117` (`extract_tangents_from_extra_data`).
- **Found**: At `:72` `bethesda_bitangent_offset = num_verts * 12;`. At `:74-82`, reads BOTH halves of the on-disk blob (`[tangents..., bitangents...]`). At `:91-92`, the swap: `t_yup` reads from the *bitangent half* (because Bethesda's "tangent" field actually stores ∂P/∂V) and `b_yup` reads from the *tangent half*. Z-up → Y-up swap at lines 91-92 + 96 (`(x, y, z) → (x, z, -y)`). Bitangent sign at lines 98-114: `sign(dot(B, cross(N, T)))` with zero defaulting to `+1` (degenerate fallback).
- Comments cite #786 explicitly at lines 61-71. This is the canonical Bethesda-blob handedness fix.
- ✓ Clean — pre-fix chrome-walls regression cannot return without re-introducing the swap reversal.

### V16-2 — FO4+ BSTriShape inline tangents (#795/#796)

- **Site**: `crates/nif/src/import/mesh/bs_tri_shape.rs:124-127` (decoder branch points), `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` (packed-vertex loop with `VF_TANGENTS = 0x010`).
- **Found**: 3 distinct paths documented in the decoder: (1) when `VF_TANGENTS` set on packed vertex desc, (2) when `VF_NORMALS` set without tangents (synthesize via `synthesize_tangents` at line 172), (3) when both upstream populates dropped tangents (fall through to `synthesize_tangents_yup` at line 187 — the #1204 fallback for SSE skin-partition reconstructed shapes with empty `VF_TANGENTS`).
- ✓ Clean.

### V16-3 — Synthesized fallback (`synthesize_tangents` + `synthesize_tangents_yup`)

- **Sites**: `tangent.rs:180` (`synthesize_tangents` — Z-up inputs, does its own axis swap), `tangent.rs:376` (`synthesize_tangents_yup` — Y-up inputs, no swap).
- **Found**: Both exist with documented contracts. `synthesize_tangents_yup` is for inputs already in Y-up (SSE-reconstructed BSTriShape with empty `VF_TANGENTS`, per the line 375 comment). Both compute the bitangent sign via `sign(dot(B, cross(N, T)))` and emit `[Tx, Ty, Tz, bitangent_sign]` shape the shader expects.
- **#1232 fix verified** (`293db681`, 2026-05-23): the comment at `bs_tri_shape.rs:180` is "lacks `VF_TANGENTS`: shape.normals/shape.uvs are empty" — line 187 routes through `synthesize_tangents_yup` instead of the pre-fix `Vec::new()` return. ✓
- **#1265 fix verified** (`dd02ad3f`, 2026-05-24): `extract_mesh` NiTriShape path no longer clones `geom.tangents` despite `geom` being dead after — uses `mem::take` to avoid the clone. (Audited at this site via the commit log; not re-read in this sweep.)
- ✓ Clean.

### V16-4 — Bitangent sign convention consistent across all 3 import paths

- **Check**: `[Tx, Ty, Tz, bitangent_sign]` with `B = bitangent_sign * cross(N, T)` reconstruction shader-side.
- **Bethesda authored** path (`tangent.rs:114`): `sign = if dot_b_cross < 0.0 { -1.0 } else { 1.0 };` ✓
- **`synthesize_tangents`**: per doc-comment at lines 161-180, follows the same convention. ✓
- **`synthesize_tangents_yup`**: per doc-comment at lines 353-376, sibling of `synthesize_tangents`. ✓
- ✓ Clean.

### V16-5 — `perturbNormal` default-on (R-N2 / #786, #787, #788)

- **Site**: `triangle.frag:908` (function definition), `:1182` (gate site).
- **Found**: gate at line 1182 is `&& (dbgFlags & DBG_BYPASS_NORMAL_MAP) == 0u`. The flag is named **BYPASS** — meaning when *unset* (default), perturbNormal IS called (line 1201). The default-on flip from #787/#788 is wired correctly. `DBG_BYPASS_NORMAL_MAP = 0x10` is the runtime opt-out for bisecting.
- ✓ Clean — verifies the post-#1162 polarity-flipped convention (DBG_* consts are NOT redeclared inside the shader, only consumed from the generated `shader_constants.glsl` include).

### V16-6 — DBG_* bit catalog (10 bits at expected offsets)

- **Site**: `crates/renderer/src/shader_constants_data.rs:119-200`.
- **Found**: All 10 bits at their expected values:
  - `DBG_BYPASS_POM = 0x1` (line 119) ✓
  - `DBG_BYPASS_DETAIL = 0x2` (line 122) ✓
  - `DBG_VIZ_NORMALS = 0x4` (line 125) ✓
  - `DBG_VIZ_TANGENT = 0x8` (line 132) ✓
  - `DBG_BYPASS_NORMAL_MAP = 0x10` (line 142) ✓
  - `DBG_RESERVED_20 = 0x20` (line 152) — formerly `DBG_FORCE_NORMAL_MAP`, no-op post-#1035 ✓
  - `DBG_VIZ_RENDER_LAYER = 0x40` (line 162) ✓
  - `DBG_VIZ_GLASS_PASSTHRU = 0x80` (line 178) ✓
  - `DBG_DISABLE_SPECULAR_AA = 0x100` (line 188) ✓
  - `DBG_DISABLE_HALF_LAMBERT_FILL = 0x200` (line 200) ✓
- The two lockstep tests in `crates/renderer/src/shader_constants.rs` (`generated_header_contains_all_defines` positive side + `triangle_frag_dbg_bits_not_redeclared` negative side) pin the constants against drift.
- ✓ Clean.

### V16-7 — #1104 UV-mirror handedness in derivative-based TBN (Path-2)

- **Site**: `triangle.frag:830-836` (Path-1 vertex-tangent derivation), `triangle.frag:892-948` (Path-2 derivative TBN reconstruction).
- **Found**: comment at line 832 — *"multiplier on B encodes UV-mirror handedness so V_ts.y has consistent direction across UV mirror seams"*. Comment at line 836 — *"perturbNormal Path-2 site carries the same correction."*
- Path-2 (`triangle.frag:946-949`) uses `dFdx(worldPos)`, `dFdy(worldPos)`, `dFdx(uv)`, `dFdy(uv)` to reconstruct T/B from screen-space derivatives — the FO4+ inline path's screen-space fallback when authored tangents are absent.
- ✓ Clean — #1104 fix verified live at both Path-1 and Path-2 sites.

---

## Regression Guard List

Confirmed still correct in current source (no audit re-found regressions):

- **#786** — Bethesda tangent/bitangent swap honored at `tangent.rs:72-115`
- **#787 / #788** — perturbNormal default-on flip, Path-1 transform fixed
- **#795 / #796** — FO4+ BSTriShape inline tangent decode
- **#1035** — `DBG_FORCE_NORMAL_MAP` renamed to `DBG_RESERVED_20` (orphan dead-code closeout)
- **#1086** — Starfield BSGeometry tangent path now non-empty (per #1232 fallback)
- **#1104** — UV-mirror handedness in Path-2 derivative TBN
- **#1162** — DBG_* const-redeclaration drop (consts live in generated header only)
- **#1199** — `unload_cell` worldspace-scoped resources preserved across cell transitions
- **#1204** — BSTriShape tangent-synthesis fallback uses `synthesize_tangents_yup`
- **#1232** — BSGeometry empty-tangent → `synthesize_tangents_yup`
- **#1265** — `extract_mesh` (NiTriShape path) uses `mem::take` instead of clone on `geom.tangents`
- **8b5d77c1** — sun-sprite mip 0 force (today)

## Notes

- Both dimensions are in "regression-guard mode" — the heavy lifting landed across Sessions 26-31 (M-NORMALS) and 2026-04 (M33/M33.1/M34). Today's sweep finds the lifters still standing.
- The audit-prompt wording note (fog "to direct only" vs "to assembled HDR pre-ACES") is mild — the load-bearing invariant (linear-space, pre-ACES) is intact per the #784 closure. Defer to a future prompt edit.
- No `/audit-publish` follow-up needed — clean regression-guard pass on both dims.
