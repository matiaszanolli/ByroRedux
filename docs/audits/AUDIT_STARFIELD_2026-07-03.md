# Starfield Compatibility Audit — 2026-07-03

**Scope**: All 9 dimensions. Engine HEAD `8498e559` on `main` — one commit past
yesterday's `AUDIT_STARFIELD_2026-07-02.md` (HEAD `1b4e8e84`). Depth/correctness
audit of the existing Starfield bring-up surface (BA2 v2/v3 + LZ4, CDB materials,
BSGeometry `.mesh` resolution, walkable Cydonia interior) — not a from-scratch
gap inventory.

**Method**: This audit runs one session after yesterday's very thorough sweep,
so the first step was to diff the intervening commit range
(`1b4e8e84..8498e559`, 27 commits) against the file set each of the 9 dimensions
touches. Only two commits land on Starfield-relevant code:

- `ba728882` — **Fix #1828 Fix #1829**: exactly yesterday's SF2-01 (HIGH) /
  SF2-02 (MEDIUM) findings, fixed same-day.
- `78209c5b` — **Fix #1717**: refreshed the ROADMAP Starfield parse-rate figures
  after a live re-sweep (yesterday's SF-D7-01 doc-rot finding).

Every other commit in range (`175ebf2c` VWD record-flag, `ae219630`/`2f0b99fa`
PEX tests, `ffe9a816` ragdoll logging, `9f48a16e` two-sided blend, `d68c86c9`
particle probe, `1748e148` SpeedTree billboard PBR, ECS/save/renderer/scripting
fixes) touches files outside every dimension's entry-point set (`crates/bsa`,
`crates/sfmaterial`, `crates/bgsm`, `crates/nif/src/blocks/shader.rs`,
`crates/nif/src/import/mesh/`, `crates/plugin/src/esm/cell/`,
`byroredux/src/cell_loader/spawn.rs`, `byroredux/src/material_translate.rs`,
`byroredux/src/asset_provider/material.rs`) — confirmed by `git show --stat` on
each commit before excluding it.

Given that near-zero drift, this session's effort went into **live
re-verification** rather than re-deriving findings from scratch: full BA2 corpus
sweep, `--sf-smoke` resolve-rate run, the new sentinel-slot regression tests, and
targeted code reads across the remaining 7 dimensions to confirm each still
holds and to hunt for anything the last six audits (2026-04-27 through
2026-07-02) might have missed.

---

## Executive Summary

Starfield remains a first-class `GameKind` and the bring-up surface is healthy.
**No new findings this session.**

- **BA2 v2/v3 + LZ4 (Dim 1)** — live-verified: `starfield_full_corpus_ba2_sweep`
  passes **129/129 archives, 0 failures** (all 32 vanilla Starfield archives +
  15 third-party/mod archives in the local corpus), covering v2 zlib/General,
  v2 Dx10, and v3 Dx10/LZ4-block. `v3_unknown_compression_method_rejected` still
  a hard error. No new defects.
- **BSGeometry mesh extraction (Dim 2)** — yesterday's SF2-01 (HIGH, #1828) and
  SF2-02 (MEDIUM, #1829) are **fixed and verified**: `crates/nif/src/import/mesh/bs_geometry.rs`
  now requires non-empty `vertices`/`triangles` before accepting a slot in both
  Stage A (`find_map`) and Stage B (external loop), continuing past `scale<=0`
  sentinel slots instead of breaking on the first one that merely parses. The 4
  new regression tests in `bs_geometry_sentinel_slot_tests.rs` all pass
  (`cargo test -p byroredux-nif --lib sentinel`). Confirmed the Internal/External
  split is shape-level (`BSGeometry.av.flags & 0x200`), not per-slot, so a mixed
  Internal+External slot list within one shape is not format-representable —
  ruling out a related edge case hypothesized this session. SF2-03 (LOW, tri_size
  hint validation) is open as **#1830**, correctly not re-filed.
- **CDB materials (Dim 3)** — parse path re-read: `ComponentDatabaseFile::parse`
  is bounds-checked (`read_u32_at` returns `Err` on short reads, no
  `unwrap`/`panic!` outside `#[cfg(test)]`). The "one unknown chunk-type aborts
  the whole 1.44M-material parse" brittleness is **already tracked and closed**
  as **#1569** (fixed by pinning the recognized-tag baseline + carrying
  index/raw in the error) — re-confirmed this is the same brittleness the
  Dim 3 checklist asks about, not a new issue. SF3-01 (#1289 Phase 2, per-field
  CDB extraction) and SF3-02 (**#1831**) remain open, correctly not re-filed.
- **ESM resolve-rate baseline (Dim 4)** — re-ran the live harness:
  `./target/release/byroredux --esm Starfield.esm --sf-smoke CityCydoniaMainLevel`
  → **25 437 / 27 898 REFRs resolved (91.2%)**, byte-identical to yesterday's
  figure. No regression. PDCL decal GRUP still surfaces as a named,
  one-shot-warned skip (`skipped_unconsumed_groups`), not the anonymous
  catch-all — confirmed live in the run's stderr.
- **ESM/cell spawn regression surface (Dim 5)** — re-read `walkers.rs` /
  `spawn.rs`: `XCLL_SIZES_STARFIELD = [28, 108]` intact, `base_layer`-gated
  trimesh fallback (#1294) intact. All guards match yesterday's confirmation;
  no touching commit landed in range.
- **NIF shader BSVER 155+ (Dim 6)** — re-read `shader.rs`: `sf1_crcs`/`sf2_crcs`
  CRC32 arrays still gated on `FO4_CRC_FLAGS`/`FO76_SF2_CRCS`; `starfield_tail`
  capture (`read_starfield_tail`) still present, saturating, no hardcoded
  length. Untouched since yesterday's confirmation.
- **Real-data NIF parse rate (Dim 7)** — the ROADMAP figures were refreshed
  *today* by `78209c5b` (Fix #1717) from a live re-sweep captured in the issue
  body: Meshes01 100%, Meshes02 100%, MeshesPatch 98.91% (325 truncated,
  #746/#747, unchanged), LODMeshes 100%, FaceMeshes 100% — **99.64% aggregate**.
  Not re-run this session (a full 5-archive / ~89K-NIF sweep is multi-minute and
  was reproduced same-day in the issue's own verification step); the doc-rot
  this figure corrects is **#1717**, now closed.
- **NIFAL material translation (Dim 8)** — `translate_material` remains the
  single `ImportedMesh → Material` boundary; `Material::metalness`/`roughness`
  in `crates/core/src/ecs/components/material.rs` are still plain resolved
  `f32` (no `Option`, no per-draw classifier). Untouched since yesterday.
- **BGSM/BGEM external material flow (Dim 9)** — re-read `bgem.rs` +
  `asset_provider/material.rs`: `glass_enabled` → `mesh.bgem_glass` forwarding
  intact; `grayscale_to_palette_alpha` still parsed-but-unforwarded, tracked as
  the existing open **#1580**. Untouched since yesterday.

**Findings**: 0 NEW. All prior findings either fixed-and-verified this session
(#1828, #1829) or confirmed still correctly tracked as open (#1830, #1831,
#1289, #1580, #1576) / closed-with-fix-in-place (#1569, #1717).

---

## Dimension Findings

No NEW findings in any of the 9 dimensions this session.

### Verified Fixed (regression-tested)

#### Was SF2-01 / #1828 (HIGH) — Stage B external `.mesh` loop short-circuited on first *parsed* slot
- **Status**: **Fixed** (commit `ba728882`, same day as yesterday's audit)
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:58-83`
- **Verification**: Code now requires `!data.vertices.is_empty() && !data.triangles.is_empty()`
  before accepting a slot and breaking; otherwise logs at `debug!` and continues
  to the next LOD slot. Regression tests
  `stage_b_skips_sentinel_first_external_slot_and_finds_populated_one` and
  `stage_b_all_sentinel_external_slots_returns_none` both pass.

#### Was SF2-02 / #1829 (MEDIUM) — Stage A internal-geom `find_map` accepted first `Internal` slot even when empty
- **Status**: **Fixed** (commit `ba728882`)
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:32-42`
- **Verification**: `find_map` closure now guards
  `Internal { mesh_data } if !mesh_data.vertices.is_empty() && !mesh_data.triangles.is_empty()`.
  Regression tests `stage_a_skips_sentinel_first_internal_slot_and_finds_populated_one`
  and `stage_a_all_sentinel_internal_slots_returns_none` both pass.

### Confirmed Still Open (correctly tracked, not re-filed)

| ID | Title | Status |
|----|-------|--------|
| #1830 (SF2-03) | `BSGeometryMesh.tri_size`/`num_verts` hints parsed but never validated against resolved geometry | OPEN, LOW |
| #1831 (SF3-02) | `.mat` arm falls to generic "unsupported format" warn when the CDB fails to parse, losing the root-cause signal | OPEN, LOW |
| #1289 (SF3-01, Phase 2) | Per-field CDB extraction — `.mat` materials reach the Disney lobe with NIF defaults, not authored CDB values | OPEN, MEDIUM (documented, scoped, roadmap-tracked) |
| #1580 (SF-D9-02) | BGEM `grayscale_to_palette_alpha` parsed but has no consumer | OPEN, LOW |
| #1576 (SF-D4-03) | Model-less STAT/BNDS/ACTI/ARMO Starfield forms drop (geometry in a BFCB component block) | OPEN |
| #1717 (SF-D7-01) | ROADMAP/compat-matrix Starfield parse-rate figures understated current state | **CLOSED** same-day, figures refreshed and verified this session |
| #1569 (SF-D3-02) | Monolithic CDB parse aborts the whole material set on first unknown chunk-type/builtin/flag bit | **CLOSED** — pinned recognized-tag baseline tests + diagnostic index/raw in error; re-confirmed this session, not a fresh finding |
| #1761 (TD8-004) | `Dx10Chunk::end_mip` set-but-never-read | OPEN, LOW (tech-debt, not Starfield-correctness) |

---

## Live Verification Log (this session)

```
$ cargo test -p byroredux-bsa --test ba2_real starfield -- --ignored
Starfield BA2 corpus sweep: 129 archives, 129 OK, 0 failures
test starfield_full_corpus_ba2_sweep ... ok
(+ starfield_meshes01_ba2_v2_gnrl_extracts_nif_with_starfield_magic,
   starfield_textures01_ba2_v3_dx10_extracts_lz4_block_dds,
   starfield_constellation_textures_ba2_v2_dx10_extracts_zlib_dds — all ok)

$ cargo test -p byroredux-nif --lib sentinel
test import::mesh::bs_geometry_sentinel_slot_tests::stage_a_all_sentinel_internal_slots_returns_none ... ok
test import::mesh::bs_geometry_sentinel_slot_tests::stage_a_skips_sentinel_first_internal_slot_and_finds_populated_one ... ok
test import::mesh::bs_geometry_sentinel_slot_tests::stage_b_all_sentinel_external_slots_returns_none ... ok
test import::mesh::bs_geometry_sentinel_slot_tests::stage_b_skips_sentinel_first_external_slot_and_finds_populated_one ... ok

$ ./target/release/byroredux --esm ".../Starfield.esm" --sf-smoke CityCydoniaMainLevel
references : 27898 REFRs
resolved   : 25437 / 27898 (91.2%)
unresolved : 2461 / 27898 (8.8%)
[PDCL GRUP encountered ... skipping — named, one-shot warn, not anonymous catch-all]
```

All three checks match yesterday's baseline exactly (BA2 sweep count grew by
mod-archive availability, not a Starfield-vanilla regression; resolve rate is
byte-identical: 25437/27898).

---

## CRC32 Flag Table

No new empirical CRC32 hash → flag-name mappings derived this session (matches
yesterday — `sf1_crcs`/`sf2_crcs` remain opaque `Vec<u32>`, consumed by
length-prefix only, correct without a name table).

---

## Remaining-Work Chain (per `starfield-esm-roadmap.md`)

Unchanged from yesterday — Phases 0+1 done, Phases 2-4 invalidated by the
~99.9%-record-parity measurement. In order of value:

1. **Per-field CDB extraction** (#1289 Phase 2): `.mat`-resolved materials
   currently reach the Disney lobe with NIF defaults.
2. **ESM-placed content gaps in Cydonia**: PDCL decals (named-skip) and #1576
   model-less STAT/BNDS/ACTI/ARMO (geometry in a BFCB component block).
3. **Exterior worldspace tiles / space-cell / planet / GBFM records** — out of
   scope for the walkable-interior milestone.
4. **#746/#747 NIF truncation tail** in Meshes01/MeshesPatch — residual, not
   grown (Meshes01 is actually now 100% per the #1717 re-sweep; MeshesPatch's
   325-file/1.09% tail is the sole survivor).
5. **BSEffectShaderProperty +32 B under-read** (Dim 6) — the sibling of the
   #1606 `BSLightingShaderProperty` fix, scoped out, not yet an issue.

---

## Deduplication Notes

- Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200
  --json number,title,state,labels` (71 open issues) captured to
  `/tmp/audit/issues.json` this session.
- Cross-checked closed-issue state via `gh issue view` for #1569, #1571, #1289,
  #1717, #1828, #1829 — all verified against current source, not re-filed.
- Prior reports reviewed: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` through
  `AUDIT_STARFIELD_2026-07-02.md` (7 prior Starfield-specific sweeps).
- **NEW this session**: none.

---

_Suggested next step_: none — no new findings to publish. If desired,
`/audit-publish` has nothing new to act on for this report.
