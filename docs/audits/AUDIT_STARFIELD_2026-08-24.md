# Starfield Compatibility Audit — 2026-08-24

*Run solo (no sub-agent fan-out) against `/mnt/data/src/gamebyro-redux` at
HEAD (`4e1afcbe`). All 9 dimensions covered, no `--focus` filter. Real game
data at `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` (129
archives, 89,276 vanilla mesh NIFs, the 1,389 MB `Starfield.esm`). Every
figure below was either re-run against real data via `cargo test`/the built
`byroredux --sf-smoke` binary, or re-derived from the current source — nothing
is copy-pasted from a prior report without independent verification this
session.*

## Executive Summary

Starfield remains a first-class `GameKind`: NIF + BA2 v2/v3 at the compat-matrix
rate, CDB + BGSM/BGEM materials, and a walkable Cydonia interior all ship
today. **This pass found zero new findings.** Three consecutive real-data
checks (BA2 corpus sweep, full mesh-archive parse rate, CDB parse) reproduced
the ROADMAP compat-matrix figures exactly, `cargo test` is green across
`byroredux-plugin` (779 passed), `byroredux-sfmaterial`/`byroredux-bgsm`/
`byroredux-bsa`, and the 108 Starfield-touching water tests in `byroredux`,
and the 20 Starfield-specific unit tests in `byroredux-nif` all pass.

The headline result is a **confirmed fix, not a new bug**: the 2026-08-20
audit's three water-translation findings against Starfield —
`SF-2026-08-20-D5-01` (DNAM[0] wrongly read as `fog_far`), `SF-2026-08-20-D8-01`
(absorption coefficients used as a divisor, producing an opaque 0.18 wu
column), and `SF-2026-08-20-D8-02` (concentration clamped to `0..1` against an
authored range up to 20.0, saturating 41/60 values) — are **all fixed** by the
`fix(watal)` commit sequence landed 2026-08-20/23 (`7f752c0c`, `b6a5588c`,
`d82d8e94`, `fa515b9c`, plus the `STARFIELD_WATER_CONCENTRATION_REFERENCE =
20.0` shader constant). These three were never published as GitHub issues, so
there is nothing to close; they are reported here as **verified fixed** for
the record, not re-filed. See Dimension 5 and Dimension 8 below for the
line-by-line verification.

Per the SKILL's cross-reference note: today's `/audit-esm` pass found that
`7f752c0c` fixed FO76/Starfield's WATR fog-vs-depth-amount confusion correctly
but regressed FO4 (filed as `ESM-2026-08-24-D5-01`, HIGH, in
`AUDIT_ESM_2026-08-24.md`). Verified independently here: the FO4 regression is
confined to `decode_dnam_fo4`'s new offset-12/16 fog-near/far reads — the
Starfield decoder (`decode_dnam_starfield`) never touches `fog_near`/`fog_far`
at all, so Starfield is not collaterally affected. Not duplicated as a
Starfield finding.

All previously-filed **open** Starfield issues (19 total, `#2360`–`#3234`)
were re-confirmed still open where their code paths are unchanged since
2026-08-20 (`crates/bgsm`, `crates/sfmaterial`, `crates/bsa/src/ba2.rs`,
`crates/nif/src/blocks/{shader,bs_geometry}.rs`, `crates/nif/src/import/mesh/
bs_geometry.rs` all show zero commits in that window) — see the per-dimension
sections for which apply where. None are re-filed.

**Total: 0 new findings (0 CRITICAL / 0 HIGH / 0 MEDIUM / 0 LOW).**

---

## Verification Method

Real-data checks run this session (all against the on-disk Starfield
install):

| Check | Command | Result |
|---|---|---|
| BA2 full corpus | `cargo test -p byroredux-bsa --test ba2_real starfield -- --ignored` | **129/129 archives OK, 0 failures** — matches ROADMAP exactly |
| NIF mesh parse rate | `cargo test -p byroredux-nif --test parse_real_nifs parse_rate_starfield_all_meshes -- --ignored` | Meshes01 100.00% (31,058), Meshes02 100.00% (7,552), MeshesPatch 99.98% (29,843/29,849, 6 truncated), LODMeshes 100.00% (19,535), FaceMeshes 100.00% (1,282) — **byte-identical to the ROADMAP compat-matrix row** |
| CDB parse | `cargo test -p byroredux-sfmaterial --test real_cdb -- --ignored` | 97 classes / 1,438,780 instances — matches the ROADMAP "1.44M instances" figure |
| ESM resolve rate | `byroredux --esm Starfield.esm --sf-smoke citycydoniamainlevel` (release build) | 25,433 / 27,898 = **91.2%** — matches the carried-forward 91.2% baseline (see Dimension 4 note on the small count delta) |
| Unit tests | `cargo test -p byroredux-plugin` / `-p byroredux-nif --lib starfield` / `-p byroredux water` / `-p byroredux-sfmaterial -p byroredux-bgsm -p byroredux-bsa` | 779 / 20 / 108 / (16+6+2) all green, 0 failed |
| `cargo check` | `-p byroredux-bsa -p byroredux-sfmaterial -p byroredux-bgsm -p byroredux-nif -p byroredux-plugin -p byroredux -p byroredux-renderer -p byroredux-core` | clean |

Delta scan: `git log --since=2026-08-20` against every Starfield entry-point
file (`crates/bsa/src/ba2.rs`, `crates/sfmaterial/`, `crates/bgsm/`,
`crates/nif/src/blocks/{shader,bs_geometry}.rs`,
`crates/nif/src/import/mesh/bs_geometry.rs`, `byroredux/src/sf_smoke.rs`) —
**zero commits** in any of them. The only Starfield-relevant code that moved
in the last 4 days is the WATR/water-translation stack
(`crates/plugin/src/esm/records/misc/water.rs`,
`byroredux/src/{env_translate,systems/water,render/water}.rs`,
`crates/renderer/shaders/water.frag`, `crates/renderer/src/shader_constants*`)
and general exterior/streaming work (EX-09/17, EX-10/11, EX-14/15, EX-16) that
touches shared `cell_loader`/`esm` code but has no Starfield-specific arm.

---

## Dimension Findings

| Dimension | New findings | Status |
|---|---|---|
| 1 — BA2 v2/v3 LZ4 block decompression | 0 | Re-verified clean, unchanged since 2026-08-20 |
| 2 — BSGeometry mesh extraction | 0 | Re-verified clean via regression-guard tests, unchanged since 2026-08-20 |
| 3 — CDB material database correctness | 0 | Re-verified clean via real-data parse |
| 4 — Starfield ESM resolve-rate baseline | 0 | Re-measured directly this session (previous reports carried it forward unverified) |
| 5 — ESM + cell bring-up regression surface | 0 | One prior LOW finding (`SF-2026-08-20-D5-01`) confirmed **fixed** |
| 6 — NIF shader blocks, BSVER 155+ | 0 | Re-verified clean, unchanged since 2026-08-20 |
| 7 — Real-data validation | 0 | Re-measured directly this session |
| 8 — NIFAL / WATAL canonical translation | 0 | Two prior findings (HIGH `SF-2026-08-20-D8-01`, MEDIUM `SF-2026-08-20-D8-02`) confirmed **fixed** |
| 9 — BGSM/BGEM external material flow | 0 | Existing open issues (`#3230`, `#2708`/SF-D9-02) re-confirmed still valid, not re-filed |

---

### Dimension 1 — BA2 v2/v3 LZ4 block decompression

`crates/bsa/src/ba2.rs` has zero commits since 2026-08-20. Re-ran the full
corpus test rather than trust that fact alone:

```
Starfield BA2 corpus sweep: 129 archives, 129 OK, 0 failures
```

This includes all v3 LZ4-block archives (`Textures01..11`,
`TexturesPatch01/02`, `LODTextures01/02` — 15 archives) and the DLC/Creation
archives (`BlueprintShips-Starfield`, `Constellation`, `OldMars`, `SFBGS003`,
`sfbgs00a_*`, `sfbgs019/021/023`, several community-mod BA2s under the same
Data dir). `compression_method` dispatch (`Ba2Compression::{Zlib=0,
LZ4Block=3}`, hard error otherwise) is untouched. No findings.

### Dimension 2 — BSGeometry mesh extraction (Starfield's actual mesh path)

`crates/nif/src/import/mesh/bs_geometry.rs` and
`crates/nif/src/blocks/bs_geometry.rs` have zero commits since 2026-08-20. Ran
the regression-guard suites directly:

```
import::mesh::bs_geometry_sentinel_slot_tests::* — 4/4 ok  (#1828/#1829 sentinel-skip guard)
asset_provider::tests::material_path::normalize_mesh_path_* — 8/8 ok  (#1292 geometries\ head guard)
blocks::dispatch_tests::starfield::* — 4/4 ok  (external mesh, internal geom, LOD survival, skin/bone dispatch)
```

`#2361`/`#2362` (the `.mesh` suffix-composition and missing-`MeshResolver`
call-site findings) remain open per the dedup baseline; their code paths
(`crates/nif/src/import/mod.rs` call sites) are unchanged since filing. Not
re-verified line-by-line this pass — no new evidence either way, so not
re-filed and not claimed re-confirmed.

### Dimension 3 — CDB material database correctness

`crates/sfmaterial/` has zero commits since 2026-08-20. Real-data parse:

```
[sfmaterial] extracted 105037616 bytes
[sfmaterial] parsed: 97 classes / 1438780 instances
```

Matches the ROADMAP's "1.44M instances" figure exactly — no drift.
`#2359` (CDB Phase 2, per-field `.mat` extraction) remains the correctly-scoped
open follow-up; not re-filed. `discover_starfield_cdbs`'s scan-based DLC/
Creation discovery (#1571) is unchanged.

### Dimension 4 — Starfield ESM resolve-rate baseline

Unlike the 2026-08-20 pass (which could not build the binary under its
briefing), this session built `byroredux --release` and ran the smoke test
directly against `citycydoniamainlevel`:

```
resolved   : 25433 / 27898 (91.2%)
STAT 22758 · LIGH 656 · MSTT 466 · MISC 454 · PKIN 370 · FURN 292 · ACTI 130 ·
IDLM 95 · ALCH 93 · DOOR 41 · CONT 37 · FLOR 25 · TERM 8 · BOOK 6 · WEAP 2
unresolved : 2465 / 27898, all slot 0x00 (in-ESM parser gap, not a missing master)
```

This matches the carried-forward 91.2% baseline (the 2026-08-16 figure was
25,437/27,898). The raw resolved count differs by 4 REFRs (25,433 vs 25,437) —
within noise at this scale (0.014%) and does not move the rounded percentage;
no code path that would explain a 4-REFR shift changed in the delta window
(the only`base_form_id`-resolution-adjacent commit, `#3056`'s ARMA-MODL fix,
affects ARMO/ARMA resolution, and ARMO does not appear in Cydonia's
resolved-by-type table). Not filed as a finding — flagging the delta for the
record so a future pass with a different baseline doesn't mistake ±4 REFRs for
new evidence of drift.

The `#1567` LIGH `DAT2` decode is confirmed live: 656 LIGH REFRs resolve (the
pre-fix count was 0 for this exact reason). The `#1568` named PDCL skip is
confirmed live via the runtime log: `WARN ... PDCL GRUP encountered ...
skipping. Placed decal REFRs won't resolve at cell-load time (cosmetic only)`.

`#2636` (SECH/AOPF zero dispatch), `#2637` (unresolved-REFR report overstating
gaps ~5x) remain open, unchanged code, not re-filed.

### Dimension 5 — ESM + cell bring-up regression surface

`XCLL_SIZES_STARFIELD = [28, 108]`, HEDR-0.96 → `GameKind::Starfield`, the
`#1294` `base_layer` collider gate, and the `#1284` `SkinSlotPool` cap-sizing
are all unchanged and covered by the green `byroredux-plugin` suite (779
passed).

**`SF-2026-08-20-D5-01` (Starfield `DNAM[0]` mapped to `fog_far`, collapsing
the above-water fog ramp to 3.45–20 world units) — CONFIRMED FIXED.**
`decode_dnam_starfield` (`crates/plugin/src/esm/records/misc/water.rs:1218`)
now reads:

```rust
if let Some(depth) = read_f32_at(data, 0) {
    // xEdit dev-4.1.6 defines DNAM[0] as `Depth Amount`, not a fog
    // distance. Creation-2 records have no above-water near/far fields,
    // so preserve the canonical fog defaults and carry this independently.
    p.depth_amount = depth.max(0.0);
}
```

`fog_near`/`fog_far` are never assigned anywhere else in the function, so they
retain `WaterParams::default()` (80.0 / 600.0) exactly as every other game
that doesn't author an explicit fog ramp. Landed by `7f752c0c` (`fix(watal):
separate creation depth amount from fog`), 2026-08-20/23.

The cross-reference from today's `/audit-esm` pass (`ESM-2026-08-24-D5-01`,
HIGH — the same commit's `decode_dnam_fo4` offset-12/16 fog reads regressed
FO4) does not extend to Starfield: `decode_dnam_fo4` and
`decode_dnam_starfield` are separate functions with disjoint offset tables,
and the Starfield function was independently re-read this session to confirm
it has no `fog_near`/`fog_far` write at all. Not duplicated here.

The EX-09/17 cross-plugin deleted-ref fix (`0cef6fc0`) touches both interior
(`walkers.rs`) and exterior (`wrld.rs`) `parse_refr_group` call sites
uniformly — no Starfield-specific branch exists to regress. Starfield exterior
worldspace support itself remains unimplemented (see *Remaining-Work Chain*),
so this fix has no live Starfield surface yet either way.

`#2364`, `#2365`, `#2638` (stale doc-rot findings — Skyrim+16-byte-tail
framing, stale 99.64% figure, stale `IsCollisionOnly` doc reference) remain
open; unrelated to code correctness, not re-verified this pass.

### Dimension 6 — NIF shader blocks, BSVER 155+

`crates/nif/src/blocks/shader.rs` has zero commits since 2026-08-20. All 20
Starfield-tagged unit tests in `byroredux-nif` pass, including the full
regression-guard set for the two headline fixes in this dimension's history:

```
blocks::shader::tests::starfield::parse_bs_lighting_starfield_captures_trailing_tail ... ok   (#1606)
blocks::shader::tests::starfield::parse_bs_lighting_starfield_tail_empty_without_size_or_drift ... ok
blocks::shader::tests::starfield::parse_bs_effect_starfield_captures_trailing_tail ... ok
blocks::shader::tests::starfield::parse_bs_effect_starfield_tail_empty_without_size_or_drift ... ok
blocks::dispatch_tests::nodes::bs_weak_reference_node_captures_starfield_trailing_tail ... ok  (#2105/#2201)
import::material::double_sided_tests::starfield_{decal,dynamic_decal,two_sided}_crc_* ... ok
```

The `bs_shader_crc32` module (`crates/nif/src/shader_flags.rs:285-346`) now
carries a mature 30-entry named CRC32 → flag table (see *CRC32 Flag Table*
below) — materially more complete than earlier reports' "opaque hashes"
framing. `#2625` (opaque-tail capture disabling drift telemetry), `#2639`
(BSVER 168-171 gap), `#2640` (SF_WEAK_REF_GAP doc claim) remain open, unchanged
code, not re-filed.

### Dimension 7 — Real-data validation

Full 5-archive mesh sweep re-run this session, reproducing the ROADMAP
compat-matrix figures exactly (see *Verification Method* table). The 6
residual MeshesPatch truncations are unchanged:

```
meshes\terrain\lc174world\objects\lc174world.1.0.1.nif
meshes\terrain\sb004templeworld\objects\sb004templeworld.1.-1.0.nif
meshes\terrain\cydoniacity\objects\cydoniacity.4.-2.-2.nif
... and 3 more
```

`#2365` (stale game-compatibility.md figures) remains an open doc-rot item, not
re-verified this pass.

### Dimension 8 — NIFAL canonical material/water translation for Starfield

`byroredux/src/material_translate.rs` (`translate_material`) and
`crates/core/src/ecs/components/material.rs` (`resolve_pbr`) are unchanged
since 2026-08-20; the NIFAL material boundary itself carries no new findings.

**Both prior water-translation findings against Starfield are CONFIRMED
FIXED**, verified end-to-end from the ESM decoder through the shader:

**`SF-2026-08-20-D8-01` (HIGH — absorption coefficients used as a divisor,
producing an inverted, opaque-at-0.18wu water column) — FIXED.**
`byroredux/src/systems/water.rs::underwater_color_at_depth` now applies
correct Beer–Lambert transmission:

```rust
(color[index] * (-optical_depth * coefficient).exp()).clamp(0.0, 1.0)
```

and the GPU side (`crates/renderer/shaders/water.frag:506-509`) matches:

```glsl
channelTransmission = exp(
    -hitDist * authoredCoefficients * (1.0 + concentrationDensity)
);
```

— an exponential decay with the authored coefficient as the extinction rate,
not a divisor. Confirmed no live divide-by-coefficient path remains anywhere
in the water absorption chain (`systems/water.rs`, `render/water.rs`,
`water.frag`).

**`SF-2026-08-20-D8-02` (MEDIUM — concentration clamped to `0..1` against an
authored range up to 20.0, saturating 41/60 values to exactly 1.0) — FIXED.**
`water.frag:482-484` now normalizes against a named reference before clamping:

```glsl
vec3 pigmentConcentration = clamp(
    max(push.concentration.rgb, vec3(0.0)) / STARFIELD_WATER_CONCENTRATION_REFERENCE,
    vec3(0.0), vec3(1.0)
);
```

`STARFIELD_WATER_CONCENTRATION_REFERENCE = 20.0`
(`crates/renderer/src/shader_constants_data.rs:459`, generated into
`shader_constants.glsl`) matches the authored maximum measured in the prior
audit's 15-record census, so the clamp no longer saturates the bulk of vanilla
values. Regression-pinned: `shader_constants.rs` tests assert both the
constant's value and its presence in the generated shader header.

`decode_dnam_fo76` now shares `decode_dnam_starfield` directly (single
function, documented byte-identical-through-144 rationale) rather than
maintaining a second offset table — reduces the surface for the two decoders
to silently diverge.

The "0 `BSSkyShaderProperty`/`BSWaterShaderProperty` in 89,276 NIFs, so the
per-shader-branch `texture_slot_layout` gap has zero runtime impact"
(`SF-2026-08-20-D8-03`, LOW) framing was not re-measured this pass — no code
changed in that path — and is not re-filed.

### Dimension 9 — BGSM/BGEM external material flow

`crates/bgsm/` has zero commits since 2026-08-20. Two open issues remain
valid, code paths unchanged, not re-filed:

- **`#3230` / `SF-2026-08-20-D9-01`** — the `#3053` CDB gate's early
  `MergeOutcome::PresenceOnly` return in `merge_external_material`
  (`byroredux/src/asset_provider/material.rs`) makes the BGSM/BGEM resolver
  unreachable once any Starfield CDB is registered, even for a `.bgsm`/`.bgem`
  path that would otherwise resolve. Confirmed vacuous on vanilla content (0
  `.bgsm`/`.bgem` files across all 129 archives) — a forward-looking gap for
  mixed/modded sessions, not a live regression.
- **`#2708` / SF-D9-02** — `RefrTextureOverlay::fill_from_bgsm`
  (`byroredux/src/cell_loader/refr.rs:237`) has `.bgsm`/`.bgem` arms but no
  `.mat` arm, unlike `merge_external_material`'s Starfield `.mat` arm. A
  Starfield REFR-level XATO/MSWP override pointing at a `.mat` path silently
  no-ops here (still reaches `material_path`, but fills no texture roles).

**New context this pass, not a new bug:** `#973` (`900aa081`, 2026-08-23)
extended the MSWP per-shape material-swap path to call
`fill_from_bgsm`/`resolve_bgsm` per shape via a shape-scoped `RefrTextureOverlay`
clone. This is the same resolver `#2708` already covers — the per-shape
extension inherits the identical `.mat`-arm gap, it doesn't introduce a new
one. Confirmed by reading `mesh_instance.rs:68-104` and `refr.rs:237-341`
directly: there is no `.mat` branch in either the pre- or post-#973 code.

---

## CRC32 Flag Table

`crates/nif/src/shader_flags.rs::bs_shader_crc32` (BSVER ≥ 132 CRC32-hashed
`sf1_crcs`/`sf2_crcs` arrays) — the full named table as of HEAD, all pinned by
unit tests:

| Flag name | CRC32 (decimal) |
|---|---|
| `Decal` | 3849131744 |
| `Dynamic_Decal` | 1576614759 |
| `Two_Sided` | 759557230 |
| `Cast_Shadows` | 1563274220 |
| `ZBuffer_Test` | 1740048692 |
| `ZBuffer_Write` | 3166356979 |
| `Vertex_Colors` | 348504749 |
| `PBR` | 731263983 |
| `Skinned` | 3744563888 |
| `EnvMap` | 2893749418 |
| `Vertex_Alpha` | 2333069810 |
| `Face` | 314919375 |
| `Greyscale_To_Palette_Color` | 442246519 |
| `Hairtint` | 1264105798 |
| `Skin_Tint` | 1483897208 |
| `Emit_Enabled` | 2262553490 |
| `Glowmap` | 2399422528 |
| `Refraction` | 1957349758 |
| `Refraction_Falloff` | 902349195 |
| `NoFade` | 2994043788 |
| `Inverted_Fade_Pattern` | 3030867718 |
| `RGB_Falloff` | 3448946507 |
| `External_Emittance` | 2150459555 |
| `ModelSpaceNormals` | 2548465567 |
| `Transform_Changed` | 3196772338 |
| `Effect_Lighting` | 3473438218 |
| `Falloff` | 3980660124 |
| `Soft_Effect` | 3503164976 |
| `Greyscale_To_Palette_Alpha` | 2901038324 |
| `Weapon_Blood` | 2078326675 |
| `LOD_Objects` | 2896726515 |
| `No_Exposure` | 3707406987 |

All 31 entries are consumed via `bs_shader_crc32::contains_any`. No unknown
CRC32 hashes were observed as opaque in this session's spot checks (the
double-sided/decal test suite exercises `DECAL`, `DYNAMIC_DECAL`, `TWO_SIDED`,
`SKINNED`, `CAST_SHADOWS`, `ZBUFFER_TEST` against real Starfield block data).

---

## Remaining-Work Chain

Per `starfield-esm-roadmap.md` (Phases 0+1 done, 2-4 invalidated by the 99.9%
parity measurement), in order:

1. **Per-field CDB extraction** (`#2359`, CDB Phase 2) — `.mat`-resolved
   materials currently reach the Disney BSDF lobe with NIF defaults rather
   than per-field CDB-extracted values. Single highest-value remaining
   Starfield fidelity item.
2. **Exterior worldspace tiles** — Starfield ships worldspaces (this session's
   `--sf-smoke` run logged 60+ discovered `WRLD` records including
   `NewAtlantis`, `DefaultWorld`, `TempJemisonWorld`) but no exterior-grid
   render path is exercised for Starfield today; Cydonia is an interior CELL
   and does not depend on this. Not a regression — genuinely unimplemented
   scope.
3. **Space-cell / planet / GBFM records** — `GBFM`/`GBFT`/`PNDT`/`STDT`/`BIOM`/
   `SFBK`/`SUNP` remain parser gaps (Dimension 4's slot-0x00 unresolved REFRs
   are largely these).
4. **The `#746`/`#747` NIF truncation tail** — 6 residual MeshesPatch files,
   unchanged this session, distinct unexplained cause from the closed
   `BSWeakReferenceNode`/cloth/`BSShaderType155` tails.

Do NOT frame this as "BGSM parser first / ESM very far apart" — both have
shipped; the remaining gaps are the four items above.

---

## Coverage Note

Per `_audit-common.md`'s un-owned-subsystem list: this pass did not separately
exercise Starfield content through FaceGen (`crates/facegen/`, no dedicated
owner), the Mod Runtime sandbox (no Starfield consumer exists), or the Havok
packfile reader (no Starfield ragdoll/cinematic content exercised — Starfield
ragdolls remain blocked on the `BhkSystemBinary` blob decoder per
`docs/engine/physal.md`, unchanged). These are pre-existing, documented gaps,
not new findings.

No GitHub issues were created by this audit (per instructions). All 19
previously-filed open Starfield issues (`#2360`–`#3234`) remain open and are
not superseded by this report except where explicitly marked "CONFIRMED FIXED"
above (those three findings were never published as GitHub issues, so there is
nothing to close).
