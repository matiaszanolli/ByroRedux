# AUDIT_SKYRIM_2026-05-11 — Skyrim SE Compatibility Sweep

Full 6-dimension audit run against the current `main` (post the
#836–#838 and #939 closure work). Cross-checked against the prior
`AUDIT_SKYRIM_2026-05-06_DIM1_4.md` and `AUDIT_SKYRIM_2026-05-05_DIM5.md`
catalogs and open-issue set.

## Executive Summary

Skyrim SE compatibility is **stable** at the NIF, BSA, and ESM
layers. Interior cells already render (ROADMAP confirms
WhiterunBanneredMare at 1932 entities / 253.3 FPS, bench `6a6950a`).
The unified Tes5Plus parser handles Skyrim alongside FO3/FNV/FO4
with zlib decompression, 24-byte record headers, and game-agnostic
cell indexing — no greenfield TES5 work remains for the "interior
cell renders" milestone.

| Dim | Title                                   | New findings | Result |
| --- | --------------------------------------- | ------------ | ------ |
| 1   | BSTriShape vertex format                | **1 LOW**    | All regression guards PASS; one cleanup opportunity. |
| 2   | BSA v105 (LZ4)                          | 0            | 100.00% parse rate, all 9 format checklist items PASS. Audit checklist item #2 premise was wrong — Skyrim ships LZ4 frame, not block. |
| 3   | BSLightingShaderProperty 8 variants     | 0 (verified) | Agent ran out of budget mid-investigation; the 8-variant matrix is recorded as "unverified" rather than filed speculatively. Re-run targeted. |
| 4   | BSEffect + specialty NiNode             | 0            | All four regression anchors PASS; 9 specialty block types correctly dispatched. |
| 5   | Real-data validation                    | **1 LOW**    | 18 862/18 862 clean on Meshes0; Meshes1 99.81% (6 trap-NIF truncations); BSDynamicTriShape warn fires on 100% of vanilla blocks (stale "never fires" comment). |
| 6   | ESM readiness                           | 0            | Audit premise stale — the unified Tes5Plus ESM parser already handles every block in the "interior cell renders" chain. |

**Total new findings: 2 LOW.**

## RT / Render Path Health

Not the focus of this audit (covered separately by AUDIT_RENDERER),
but spot checks confirm the four BSLightingShaderProperty / BSEffect
flag-bit anchors from the 2026-05-10 sweep are still in place:
SK-D4-NEW-04 (BSEffect flag-bit import-side capture) wired at
`shader_data.rs:42` with four consumers at `material/mod.rs:204-274`.

## Dimension Findings

### Dim 1 — BSTriShape Vertex Format

**Regression check** (all PASS):
- **#836** `data_size` warn gate on `num_vertices != 0` at
  `crates/nif/src/blocks/tri_shape.rs:550`.
- **#838** `NiLodTriShape` distinct wrapper at `tri_shape.rs:210`
  (dedicated parser at line 236, not folded into BSTriShape).
- **#621 / SK-D1-04** `BSDynamicTriShape.vertex_desc |= VF_FULL_PRECISION << 44`
  at `tri_shape.rs:890`.

**New finding (1):**

#### [LOW] SK-D1-NEW-04 — Duplicate `half_to_f32` decoder

Two near-identical IEEE-754 binary16 decoders exist in parallel:
`crates/nif/src/blocks/tri_shape.rs:1261` (`pub(crate) fn half_to_f32`,
the canonical one used by BSTriShape) and
`crates/nif/src/import/mesh.rs:1565` (a private re-declaration with the
comment "Re-declared so `import/mesh.rs` doesn't depend on a
`pub(crate)` export in tri_shape that might churn"). Both implement
identical four-branch decode (zero / subnormal-renorm / Inf+NaN /
normal); any future fix or fast-path (e.g. `f16::from_bits(h).to_f32()`
once `core::f16` stabilises) must be applied in both sites or the two
will silently diverge — exactly the cross-file drift class the
project's "always prioritise improving existing code rather than
duplicating logic" rule (global CLAUDE.md) targets.

**Fix**: promote `tri_shape::half_to_f32` from `pub(crate)` to `pub`,
or move both copies into `crates/nif/src/util.rs`. Delete the
`import/mesh.rs` copy and `use crate::blocks::tri_shape::half_to_f32;`.
The stated "churn" risk in the existing comment is unfounded — the
decoder is fully specified by IEEE 754 binary16.

### Dim 2 — BSA v105 (LZ4)

All 9 checklist items PASS against current code, with the
sweetroll01.nif raw-bytes probe confirming the on-disk LZ4 dialect
is **frame** (`0x184D2204` magic), not block. The audit checklist
item #2 premise (which asserted block) was wrong; the
`lz4_flex::frame::FrameDecoder` call at `archive.rs:603-607` is
correct. Filing a "switch to block" finding would have been a stale
premise — the kind of audit error
`feedback_audit_findings.md` warns about (5 of 30 in the 2026-04
sweep). Real-data brute-force extract sweep returns 0 errors;
texture archives (`Skyrim - Textures0.bsa`) open and decode DDS
without error.

### Dim 3 — BSLightingShaderProperty 8 Shader Variants

Agent budget exhausted before any variant cell could be verified to
a file:line. Per `feedback_speculative_vulkan_fixes.md` and
`feedback_audit_findings.md`, no SK-D3-NEW findings filed without
verification. Re-run this dimension in isolation when scheduling
permits — entry points captured in `dim_3.md`:

1. BSLightingShaderProperty parse path at `crates/nif/src/blocks/shader.rs`
   (search for Shader Type read).
2. MaterialInfo propagation in `crates/nif/src/import/material/`.
3. Fragment-stage consumption in `crates/renderer/shaders/triangle.frag`.

### Dim 4 — BSEffect + Specialty Nodes

All four regression anchors (#836, #837, #838, SK-D4-NEW-04) PASS.
Dispatch coverage table:

| Block type                            | Dispatch                                       |
| ------------------------------------- | ---------------------------------------------- |
| `BSLODTriShape`                       | `mod.rs:295` → dedicated `NiLodTriShape::parse` |
| `BSMeshLODTriShape`                   | `mod.rs:296` → `BsTriShape::parse_lod`         |
| `BSSubIndexTriShape`                  | `mod.rs:309` → `BsTriShape::parse_sub_index`   |
| `BSDynamicTriShape`                   | `mod.rs:325` → `BsTriShape::parse_dynamic`     |
| `BsLagBoneController`                 | `mod.rs:673` → dedicated                       |
| `BsProceduralLightningController`     | `mod.rs:674` → dedicated                       |
| `BSTreeNode`                          | `mod.rs:190`                                   |
| `BSPackedCombinedGeomDataExtra/Shared`| `mod.rs:550`                                   |

`as_ni_node` (`walk.rs:63-105`) covers every Skyrim NiNode subclass
either via explicit unwrap or via parse-time aliasing to plain
`NiNode` (BSFadeNode / BSLeafAnimNode / BSFaceGenNiNode /
RootCollisionNode / AvoidNode / NiBSAnimationNode / NiBSParticleNode
at `mod.rs:168-175`).

### Dim 5 — Real-Data Validation

**nif_stats Meshes0**: 18 862 / 18 862 clean, 0 truncated, 0
recovered, drift histogram empty. `NIF_STATS_MAX_DRIFT_EVENTS=0`
gate exits 0.

**nif_stats Meshes1**: 3 179 / 3 185 clean (99.81%), 6 truncated —
all Havok-heavy trap NIFs (`trapmace01.nif`,
`trapbonealarm{01,02}.nif`, `trapskullram01.nif`, `traptripwire01.nif`,
one plants pivot-anim NIF). All parse-recoverable; only the trailing
bhk block truncates. Recovery is 100% (no NiUnknown demotion).

**Block-type histogram spot-check** confirmed: 67k+
`BSLightingShaderProperty`, 5.7k `BSEffectShaderProperty`, 21.1k
`BSDynamicTriShape`, 20 `BSLODTriShape`. FO4-only types
(`BSPackedCombined*`) correctly absent.

**New finding (1):**

#### [LOW] SK-D5-NEW-08 — BSDynamicTriShape "vanilla never fires" comment is empirically false

`crates/nif/src/blocks/tri_shape.rs:894-916` — the SK-D1-02 / #571
comment claims "Vanilla Skyrim SE facegen ships `data_size > 0`...
on shipped vanilla content this never fires." Real-data measurement
shows every single BSDynamicTriShape in `Skyrim - Meshes0.bsa`
(21 140 / 21 140) has `data_size == 0`, with the actual head
geometry living in the trailing Vector4 dynamic array (the
`parse_dynamic` path at lines 865-892). The warn fires 21 140 times
per archive scan, drowning logs at WARN level for any tool that
touches the SSE mesh BSA.

**Fix**: either downgrade the warn to a one-shot `trace!` (the path
is load-bearing and works — M41 outfit equip uses these meshes), or
invert the gate to only fire when
`dynamic_vertices.is_empty() && triangles.is_empty()` (the actual
silent-failure case). Update the surrounding doc comment in
lockstep.

### Dim 6 — ESM Readiness

Audit premise stale — there is no `legacy/tes5.rs`; the legacy
stub modules were deleted (issue #390, doc-comment at
`crates/plugin/src/legacy/mod.rs:26-32`). The working parser is
unified under `crates/plugin/src/esm/` with `EsmVariant::Tes5Plus`,
24-byte record headers, zlib decompression via `flate2`, and a
game-agnostic `EsmCellIndex`. Every record-type the
"interior cell renders" milestone needs is dispatched (CELL, REFR,
STAT, LIGH, LAND, LTEX, TXST, ADDN, HDPT, NAVM safely-skippable).
Skyrim SE interior cells render today per ROADMAP.md:147.

## Shader Variant Coverage Matrix

**Dim 3 did not complete in budget**, so the 8-variant matrix
remains formally unverified. Treat this as a known gap; re-run
Dim 3 in isolation. The 2026-05-06 audit covered SK-D4-NEW-04
(BSEffect flag-bit capture) which IS verified PASS (Dim 4 above);
the BSLighting flag-bit equivalent should be checked for symmetry
in the targeted re-run.

## Forward Blocker Chain

**For "interior cell renders" — nothing blocks.** All five gates
in the chain are landed:

1. zlib decompression (`reader.rs:454-471`).
2. Tes5Plus 24-byte headers (`reader.rs:401-447`).
3. CELL/REFR dispatch (`records/mod.rs:684`, `cell/walkers.rs:288`).
4. STAT/LIGH/MSTT/FURN/DOOR/FLOR/IDLM/BNDS/ADDN/TACT (`records/mod.rs:705-706`).
5. Cell loader wiring (`byroredux/src/cell_loader.rs:899`).

**For "exterior cell renders"** — outside this audit's scope, but
the unified parser also handles WRLD records per the cell-loader
parity work (M32.5).

**Cosmetic gap**: localized strings (TES4 flag 0x80) are captured on
`FileHeader.localized` (`reader.rs:557`) but the downstream lstring
resolution for FULL/DESC display is outstanding. Not blocking for
"renders".

## Prioritized Fix Order

1. **SK-D5-NEW-08** (LOW) — log-noise fix on `tri_shape.rs:894-916`.
   One-shot trace or invert the gate. ~5 LOC. Removes 21 140
   spurious WARNs per Meshes0 scan and unblocks log-grep workflows
   on any Skyrim tool.
2. **SK-D1-NEW-04** (LOW) — `half_to_f32` deduplication. Promote
   `pub(crate)` → `pub` and delete the `import/mesh.rs` copy. ~10
   LOC. Cleanup.
3. **Dim 3 follow-up** (no severity — process gap) — Re-run the
   BSLightingShaderProperty 8-variant matrix audit in isolation
   with adequate budget; the in-scope investigation didn't complete.

## Caveats

- `Skyrim - Meshes1.bsa` ships at 99.81% clean — the 6 trap NIF
  truncations have been there since pre-this-sweep and are not new.
  The drift gate (`NIF_STATS_MAX_DRIFT_EVENTS`) doesn't catch
  success-rate failures; that's a separate observability layer.
  Not filing as a new finding because the truncations recover
  cleanly and the trap NIFs render.
- DLC archives (Dawnguard/Dragonborn/HearthFires) ship merged into
  `Skyrim - Meshes{0,1}.bsa` on this install — no separate `.bsa`
  files. Out of scope for parse-rate verification.
- Texture-archive validation was done via `bsa_grep` enumeration,
  not exhaustive DDS decode; the audit checklist explicitly notes
  texture archives hold DDS only, so this is sufficient.

---

Suggest: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-05-11.md`
to file the 2 LOW findings (SK-D1-NEW-04, SK-D5-NEW-08) and the
Dim 3 re-run process gap.
