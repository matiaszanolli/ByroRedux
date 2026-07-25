# Starfield Compatibility Audit — 2026-07-25

Depth/correctness regression audit of ByroRedux's shipped Starfield support,
run as one leg of a `comprehensive` audit-suite sweep. Starfield is a
first-class `GameKind`: NIF + BA2 v2/v3 (LZ4-block), CDB + BGSM/BGEM
materials, and a walkable Cydonia interior all ship today. This is **not** a
from-scratch gap inventory — it targets regressions in the bring-up surface
(BA2 v3 decompress, CDB chunk index, BSGeometry `.mesh` resolution, spawn
gates, NIFAL translation) plus the remaining tracked ESM phase work.

Per explicit instruction for this run, all 9 dimensions were audited directly
in a single session (no sub-agent delegation) — real code was read, unit and
integration test suites were executed, and the headless engine plus several
`crates/nif` example tools were driven against real game data at
`/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/`.

**Total: 6 findings — 0 CRITICAL, 1 HIGH, 0 MEDIUM, 0 LOW, plus 5 reconfirmed
still-open findings from the prior (2026-07-16) audit that are unchanged and
not re-filed.**

## Executive Summary

Eight of nine dimensions are in the same good shape the 2026-07-16 audit
found them in, and this run additionally confirms that **all seven fixes**
landed since that audit (#2100–#2107, closing SF-D3-AUDIT-01/02/03,
DIM4-STARFIELD-01, DIM5-01, SF-D7-NEW-01, SF-D7-NEW-02, SF-D8-01) are
correctly in place: the CDB loader now probes the header only
(`probe_header_skips_instance_walk`), `read_primitive_string` trims embedded
NULs, `peek_magic` gates discovery, the Phase-0 baseline doc note is
annotated, the `walkers.rs` XCLL doc now matches its `>= 108` gate, the
`.mat`-arm comment is corrected, and `numeric_sibling_paths` recognizes
Starfield's two-digit zero-padded mesh series.

**However, the #2105 fix that closed SF-D7-NEW-01 (the MeshesPatch
`BSWeakReferenceNode` populated-list bug) introduced a severe new regression
in a sibling archive that its own validation never exercised.** Re-running
the project's own `parse_rate_starfield_all_meshes` real-data test (gated
`--ignored`, not part of default CI) shows `Starfield - Meshes02.ba2` — a
vanilla archive the compat matrix and the 2026-07-16 audit both recorded at
100% clean — now parses at **6.10% clean (461/7,552), with 7,091 NIFs
(93.9%) truncated**, every single one losing its `BSWeakReferenceNode` block
to `NiUnknown`. Byte-level tracing (Dimension 7, below) roots this in the
`bsver >= SF_FORM_ID` (173) gate added by #2105: real Meshes02 content is
uniformly bsver 173 and does **not** carry the undocumented 2-byte field the
fix assumes is present at that threshold, while the MeshesPatch content the
fix was built and tested against is uniformly bsver 175 and does. This is a
textbook single-gate-two-populations bug, and the fix's own regression test
only encodes the bsver-175 fixture.

Six other findings from the prior audit remain open, reconfirmed unchanged
by this session (code inspected, no drift): `LZ4-01` (#2097), `SF2D2-01`
(#2098), `SF2D2-02` (#2099), `SF2D2-03`/#1827, `SF-D9-01` (#2108), `SF-D9-02`
(#2109). None require re-filing.

No CRITICAL findings. No wrong/divergent `Material` was found escaping the
`translate_material` NIFAL boundary (Dimension 8 reconfirmed clean, 85
collision + 26 particle + 7 material-translate unit tests passing). ESM
resolve-rate (Dimension 4) reproduces the documented Cydonia baseline
byte-for-byte: 91.2% (25,437/27,898 REFRs), including the 656/656 LIGH
`DAT2` resolution.

---

## Dimension Findings

### Dimension 1: BA2 v2/v3 — LZ4 Block Decompression

All checklist items verified OK, no regressions. `cargo test -p byroredux-bsa`
(unit + real-archive `--ignored` sweep) passes clean: v2/v3 header offset
math, hard-error on unknown compression method, unified GNRL+DX10 path.
Real-data run against
`Starfield - Meshes01.ba2`/`Textures01.ba2`/Constellation Textures:
`starfield_full_corpus_ba2_sweep`, `starfield_textures01_ba2_v3_dx10_extracts_lz4_block_dds`,
`starfield_meshes01_ba2_v2_gnrl_extracts_nif_with_starfield_magic` — all 4
tests pass with real archives.

- **Existing: #2097 (LZ4-01)** — reconfirmed unchanged. `crates/bsa/src/ba2.rs:718`
  still calls `lz4_flex::block::decompress(packed, unpacked_size)` with no
  `catch_unwind` wrapper, relying on the pinned `lz4_flex` version's observed
  (not contractually guaranteed) hard-error-not-panic behavior on a
  size-mismatched payload. Not re-filed; no change since 2026-07-16.

### Dimension 2: BSGeometry Mesh Extraction

All named regression guards (#1292, #1209, #1828, #1829, #1203, #1232) intact
— `cargo test -p byroredux-nif --lib bs_geometry` (33 tests) and the
sentinel-slot / skin / tangent test modules all pass. Verified via direct grep
that the two known parser gaps are unchanged:

- **Existing: #2098 (SF2D2-01)** — `bounding_sphere` (`bs_geometry.rs:234-235`)
  still used verbatim as the mesh's local bound with no havok-scale
  cross-check. Unchanged.
- **Existing: #2099 (SF2D2-02)** — `uvs1` is still decoded
  (`crates/nif/src/blocks/bs_geometry.rs:272`) and still dropped by the
  importer (`crates/nif/src/import/mesh/bs_geometry.rs:160` only clones
  `uvs0`). Unchanged.
- **Existing: #1827 (SF2D2-03)** — `extract_skin_bs_geometry`
  (`crates/nif/src/import/mesh/skin.rs:263-264`) still returns empty
  `vertex_bone_indices`/`vertex_bone_weights` for Starfield BSGeometry actors
  (bind-pose only). Unchanged, still tracked.

### Dimension 3: CDB Material Database Correctness

All three prior findings (SF-D3-AUDIT-01/02/03) are **fixed and verified**.
`cargo test -p byroredux-sfmaterial` (14 unit/integration tests) passes,
including the new regression tests directly pinning the fixes:
`probe_header_skips_instance_walk`, `read_primitive_string_trims_nul`,
`peek_magic_gates_discovery`. Real-data re-run of `parse_vanilla_materialsbeta_cdb`
against the actual 105 MB vanilla `materialsbeta.cdb` reproduces the prior
baseline exactly: 97 classes / 1,438,780 instances, 9.63s parse time (this is
now only paid on the Phase-2 on-demand re-parse path, not at every Phase-1
presence check, per #2100's fix). CDB remains the sole vanilla Starfield
material source confirmed in this run. No new findings.

### Dimension 4: Starfield ESM Resolve-Rate Baseline

Live `--sf-smoke citycydoniamainlevel` against the real, currently-patched
`Starfield.esm` reproduces the Cydonia resolve rate exactly:
**91.2% (25,437/27,898 REFRs)**, byte-identical to the 2026-07-02/07-03/07-16
baselines. Per-base-type breakdown (STAT 22,758, LIGH 656, MSTT 466, MISC
454, PKIN 370, FURN 292, ACTI 130, …) and the `slot 0x00` unresolved-2,461
count are unchanged. The #1567 LIGH `DAT2` decode still resolves all 656
Cydonia lights. No regressions; no new findings.

### Dimension 5: ESM + Cell Bring-up Regression Surface

All seven named spawn-path regression guards plus the XCLL/PDCL/NAVM guards
reconfirmed intact: `cargo test -p byroredux-plugin --lib walkers` (12 tests,
including `starfield_xcll_sizes_pinned` and
`starfield_xcll_above_108_still_takes_sf_arm`) and `cargo test --bin byroredux
spawn` (51 tests spanning `synthesize_trimesh_tests`, `attach_points_spawn_tests`,
NPC spawn, particle spawn) all pass. Confirmed **DIM5-01 (#2104) is fixed**:
`crates/plugin/src/esm/cell/walkers.rs`'s module doc now reads `>= 108` (not
`== 108`) in both prior locations, matching the live gate and its inline
`#1579` comment three lines below. No new findings.

### Dimension 6: NIF Shader Blocks — BSVER 155+ (regression guard)

**Zero findings**, reconfirmed. `cargo test -p byroredux-nif --lib shader`
(163 tests) passes clean, including
`bs_shader_crc32_matches_nif_xml_literals` and the full CRC32/flag-name test
suite. The #1510 (`BSShaderType155`) and #1606 (`starfield_tail`) regression
guards remain intact per the unchanged, passing test set. No real-data
re-sweep was needed beyond what Dimension 7 already exercises (every mesh
archive parses through this shader-block path).

### Dimension 7: Real-Data Validation

This dimension surfaced the audit's one HIGH finding — a live, previously
undetected regression.

#### SF-D7-2026-07-25-01: The #2105 `BSWeakReferenceNode` 2-byte-gap fix truncates 93.9% of `Starfield - Meshes02.ba2` (7,091/7,552 NIFs) — regression of a fix for a different archive's bug

- **Severity**: HIGH
- **Dimension**: 7 (Real-Data Validation), regression of #2105 (SF-D7-NEW-01, 2026-07-16 audit)
- **Location**: `crates/nif/src/blocks/node.rs:911-930` (`BsWeakReferenceNode::parse_inner`, the `#2105` 2-byte skip gated on `stream.bsver() >= crate::version::bsver::SF_FORM_ID`); regression test gap at `crates/nif/src/blocks/dispatch_tests/nodes.rs:246-304` (`bs_weak_reference_node_parses_populated_lists_with_undocumented_gap`, hardcodes `user_version_2: 175`)
- **Status**: NEW (regression of closed #2105, landed commit `b7e0318f`, 2026-07-21)
- **Description**: #2105 fixed a real bug where populated `BSWeakReferenceNode`
  weak-ref lists on real `Starfield - MeshesPatch.ba2` content (325/29,849
  files, all bsver 175) were mis-parsed because an undocumented 2-byte field
  sits between the weak-ref array and `unkInt1`. The fix gates the 2-byte
  skip on `bsver >= SF_FORM_ID` (173) — the same threshold that gates the
  per-entry `formID` field. That threshold is too broad: real
  `Starfield - Meshes02.ba2` content is uniformly bsver **173** (exactly the
  gate boundary) and does **not** carry the extra 2-byte field, so the new
  skip misaligns every populated `BSWeakReferenceNode` block in that archive,
  corrupting the read of `unkInt1`/`num_water_refs` and — because the
  resulting garbage water-ref count implies a `skip()` past EOF — dropping
  the block to `NiUnknown`.
- **Evidence**:
  - `BYROREDUX_STARFIELD_DATA=... cargo test -p byroredux-nif --test parse_real_nifs parse_rate_starfield_all_meshes --release -- --ignored` fails:
    `[Starfield/Starfield - Meshes02.ba2] clean rate 6.10% (461 clean / 7091 truncated / 0 failed)`.
    The sibling archives are unaffected: Meshes01 100% (31,058/31,058),
    MeshesPatch 99.98% (29,843/29,849, matching the documented 6-file
    residual), LODMeshes 100% (19,535/19,535), FaceMeshes 100% (1,282/1,282).
  - `nif_stats --unknown-only` against `Starfield - Meshes02.ba2` confirms:
    `parsed 461 unknown 7091 type BSWeakReferenceNode` — the *only* type with
    any unknown count in the archive.
  - `trace_block` byte-level decode of three independently-sampled truncated
    Meshes02 files (`lc179world.1.-2.1.nif`, `cydoniacity.1.1.3.nif`,
    `rl036world.1.-1.-1.nif`) all show `user_version_2 (bsver): 173` and all
    fail at the exact same shape: the naive field walk (base NiNode → 1
    weak-ref entry with `formID`+transform+0 materials → `unkInt1` →
    `num_water_refs`) reads a huge garbage `num_water_refs` value 2 bytes
    into what the block's own declared `size` says should already be past the
    end of the block (e.g. one sample: declared `size=176`, but the fields as
    currently parsed only line up cleanly if the 2-byte `#2105` skip is
    *not* applied — removing it would land `consumed == 176 == size`
    exactly).
  - By contrast, `trace_block` on a `Starfield - MeshesPatch.ba2` file that
    parses cleanly today (`lc133world.1.-1.0.nif`) shows `bsver: 175` and
    consumes its declared block size exactly (8,970/8,970) *with* the 2-byte
    skip applied — confirming the skip is correct for bsver-175 content and
    wrong for bsver-173 content.
  - `Starfield - Meshes01.ba2` (100% clean, unaffected) has **no**
    `meshes\terrain\*` content at all (checked via `d5_listba2`), which is
    why the base-game archive with the same era's bsver never exercises this
    code path.
  - The regression test #2105 shipped
    (`bs_weak_reference_node_parses_populated_lists_with_undocumented_gap`)
    hardcodes `user_version_2: 175` in its synthetic fixture and asserts the
    2-byte-gap-present shape parses cleanly — there is no sibling fixture for
    the bsver-173/gap-absent shape, so the test suite could not have caught
    this before it shipped.
  - `ROADMAP.md:245` states, under a `2026-07-21 sweep` byline (the same date
    #2105 landed): `Meshes02 **100%** (7 552)` — directly falsified by this
    run. The figure was legitimately 100% when first measured (commit
    `dd203a00`, 2026-04-28) and was not re-verified against real data after
    the #2105 change landed.
- **Impact**: 7,091 of 7,552 (93.9%) NIFs in a vanilla Starfield mesh archive
  now lose their entire `BSWeakReferenceNode` payload to `NiUnknown`. Current
  player-visible/runtime impact is effectively zero — this payload
  (weak-refs, water-refs) is not yet consumed by anything (feeds the
  unbuilt M35+ LOD-streaming/packin system per the code's own doc comment),
  and the content in question (`meshes\terrain\*`) is exterior/LOD geometry,
  not the interior Cydonia cell this project's cell-loading currently
  renders. The real risk is (a) the project's own compat-matrix and prior
  audit now cite a false 100%-clean figure for a whole archive, actively
  misleading anyone reasoning about Starfield NIF coverage, and (b) even the
  461 files nif_stats calls "clean" likely still suffer the same 2-byte
  misalignment silently (their water-ref-list is probably empty, so the
  resulting garbage `num_water_refs` read happens not to overflow before the
  outer block-size-table realignment silently recovers stream position) —
  meaning this data would arrive corrupted, not just truncated, the moment a
  future consumer reads it.
- **Related**: Regression of closed #2105 / SF-D7-NEW-01 (2026-07-16 audit).
  Sibling of the already-tracked residual-6 MeshesPatch truncation (also
  `BSWeakReferenceNode`, bsver 175, but a distinct and still-unexplained
  cause per that finding's own text — unaffected by this bug).
- **Suggested Fix**: Narrow the 2-byte-gap gate to the bsver range actually
  observed to carry the field (empirically `>= 175`, not `>= SF_FORM_ID =
  173`) rather than reusing the `formID`-presence gate, since the two
  properties do not correlate 1:1 in real content. Add a second synthetic
  regression fixture at `user_version_2: 173` (mirroring Meshes02's real
  shape: 1 weak-ref entry, 0 materials, 0 water-refs, no 2-byte gap) so the
  test suite pins both populations. Until fixed, treat the ROADMAP's
  Meshes02 100% figure as stale and re-run
  `parse_rate_starfield_all_meshes -- --ignored` after any future change
  to `BsWeakReferenceNode`.

### Dimension 8: NIFAL Canonical Material Translation for Starfield

All checklist items reconfirmed OK, zero findings. `cargo test --bin
byroredux material_translate` (7 tests), `cargo test -p byroredux-nif --lib
collision` (85 tests), and `cargo test -p byroredux-nif --lib particle` (26
tests) all pass. Grep confirms zero `if game ==` / `GameKind::Starfield`
branching in `material_translate.rs` or any shader source — the single
`translate_material` boundary holds. `metalness_override`/`roughness_override`
are still set as `Some(classify_legacy_pbr(...))` at import time
(`asset_provider/material.rs:709-710,847-848`), confirming the #2107 comment
fix (SF-D8-01) accurately describes the mechanism now. No new findings.

### Dimension 9: BGSM/BGEM External Material Flow

Both prior findings reconfirmed unchanged, code inspected directly:

- **Existing: #2108 (SF-D9-01)** — `cell_loader.rs:248`
  (`pack_bgsm_material_flags`) still gates `EFFECT_PALETTE_COLOR` purely on
  `mesh.bgsm_greyscale_lut_path.is_some()`, not the authoritative
  `grayscale_to_palette_color` flag (which is parsed at
  `crates/bgsm/src/bgem.rs` and only consulted elsewhere, at
  `asset_provider/material.rs:134`, in an unrelated glass-classification
  guard — never wired into the flag-pack gate itself). Unchanged.
- **Existing: #2109 (SF-D9-02)** — confirmed `glass_fresnel_color`,
  `glass_refraction_scale_base`, `glass_blur_scale_base`,
  `glass_roughness_scratch`, `glass_dirt_overlay`,
  `environment_mapping_mask_scale` (all defined in `crates/bgsm/src/bgem.rs`)
  have zero references in `byroredux/src/asset_provider/material.rs` — still
  dropped in the BGEM merge arm. Unchanged.

`cargo test -p byroredux-bgsm` (27 unit tests) passes clean; BGEM/BGSM magic
dispatch and the `glass_enabled` opaque-misclassify guard remain intact. No
new findings.

---

## CRC32 Flag Table (BSVER ≥ 132/152 shader flag arrays)

Unchanged from the 2026-07-16 audit — reconfirmed present and pinned at
`crates/nif/src/shader_flags.rs:235-310` (33 named constants,
`bs_shader_crc32_matches_nif_xml_literals` passing):

| Flag Name | CRC32 |
|---|---:|
| `DECAL` | 3849131744 |
| `TWO_SIDED` | 759557230 |
| `CAST_SHADOWS` | 1563274220 |
| `PBR` | 731263983 |
| `NO_EXPOSURE` | 3707406987 |
| `VERTEX_COLORS` | 348504749 |

---

## Remaining-Work Chain

Per `starfield-esm-roadmap.md` (Phases 0+1 done; Phases 2-4 invalidated by the
measured ~99.9% ESM record parity), the ordered remaining work is unchanged
from 2026-07-16, plus the new regression fix ahead of it:

1. **Fix SF-D7-2026-07-25-01** (this audit) — the `BsWeakReferenceNode`
   2-byte-gap gate needs a correct threshold (or a content-shape probe
   instead of a bsver threshold), and a second regression fixture at the
   bsver-173/gap-absent shape. Currently zero player-visible impact, but it
   falsifies the ROADMAP compat matrix and will corrupt data silently the
   moment the weak-ref/water-ref payload gets a real consumer.
2. **Per-field CDB extraction** (#1289 Phase 2 follow-up) — `.mat`-resolved
   materials still reach the Disney lobe with NIF-keyword-classified
   defaults (`classify_legacy_pbr`), not CDB-authored roughness/metalness/
   texture values. The Phase-1 fallback mechanism itself is correct
   (Dimension 8); only the CDB→field wiring is unbuilt.
3. **Exterior worldspace tiles** — not yet built. (Note: this is exactly the
   category of content — `meshes\terrain\*` — affected by finding #1 above,
   which is why its current runtime impact is nil.)
4. **Space-cell / planet / GBFM records** — `GBFM` remains a zero-dispatch
   stub (3,141 leaf records); `PNDT`/`STDT`/`BIOM` correctly out of scope for
   interior-cell work.

Both BGSM/BGEM parsing and the ESM pipeline are fully shipped — this remains
a depth/correctness list, not a "parser first / ESM far behind" framing.

---

## Deduplication Note

Cross-referenced all findings against `gh issue list --repo
matiaszanolli/ByroRedux` (200-issue snapshot, `/tmp/audit/issues.json`) and
`docs/audits/AUDIT_STARFIELD_*.md` (prior 9 reports, most recently
2026-07-16). Confirmed fixed since 2026-07-16: #2100, #2101, #2102, #2103,
#2104, #2105 (partially — see the new regression above), #2106, #2107.
Confirmed still open and unchanged: #2097, #2098, #2099, #1827, #2108, #2109
— none re-filed. One genuinely new finding this session
(SF-D7-2026-07-25-01), which is a regression of closed #2105 rather than a
restatement of any open issue.
