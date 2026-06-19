# Starfield Compatibility Audit — 2026-06-18

**Scope**: All 9 dimensions, run as a **regression-and-delta** pass over the
bring-up surface that the exhaustive 2026-06-14 audit certified. Engine HEAD on
`main` (post `2aac5351` BC1 punch-through alpha; post the `218b425b` shader-include
split; post `234c6f1a` FO4/FO76 XCLL reclassification; post the `d9f2dbb8`/`df60da80`/
`1605be44` NIF import refactors).
**Methodology**: Orchestrated agent fan-out (general-purpose / legacy-specialist /
renderer-specialist). Because 2026-06-14 was an exhaustive depth audit and its
HIGH/MEDIUM findings are already filed as open issues, this pass concentrated on
(a) verifying those filed gaps remain the only ones and (b) hunting for
**regressions introduced by the commit wave since 2026-06-14**. Every candidate
finding was re-read against live code and adversarially disproved before inclusion.
**Live data**: `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` PRESENT
(all 5 vanilla mesh BA2s, `Starfield - Materials.ba2` CDB, texture BA2s, `Starfield.esm`
+ DLC ESMs). Long real-data sweeps (`--sf-smoke`, `--ignored parse_rate`) were
reasoned-from-code this round rather than re-run, since no commit since 2026-06-14
touched the relevant decode paths — see per-dimension disproof.
**Dedup baseline**: `/tmp/audit/issues.json` (29 open issues). Prior reports:
`docs/audits/AUDIT_STARFIELD_2026-06-14.md` (and `_2026-05-28`, `_2026-05-18`,
`_2026-04-27`).

## Executive Summary

**Starfield's bring-up surface is regression-free across the entire commit wave
since 2026-06-14.** The recent changes — the BC1/DXT1 punch-through-alpha fix, the
`triangle.frag` → `include/*.glsl` shader split, the FO4/FO76 XCLL size
reclassification, and the NIF import coord/tangent/tint-map refactors — were each
traced to their Starfield blast radius and confirmed either SF-safe or
SF-irrelevant. The known correctness gaps remain exactly the ones already filed
(CDB per-field extraction #1290/#1289-Phase2, ESM-placed LIGH #1567, PDCL #1568,
model-less BFCB forms #1576, StarfieldLighting forwarding #1578, SF LOD shader
under-read #1606, CDB brittleness #1569/#1571).

This pass surfaces **2 NEW findings, both LOW/INFO and both hardening/doc-rot in
the GpuMaterial sync surface that the shader-include split left behind** — no
runtime defect.

Regression verification map (all CLEAN — disproof recorded per dimension below):

- **BA2 v2/v3 + LZ4** — unchanged since the 129/129 clean sweep; two-axis codec
  model (archive-wide `Ba2Compression` + per-chunk `packed_size==0` raw marker)
  intact; LZ4 `unpacked_size` is a hard bound, not a hint. **0 findings.**
- **DDS BC1 punch-through alpha (`2aac5351`)** — well-isolated and byte-count
  neutral. Both DXT1 and DXGI BC1_UNORM/SRGB now map to `BC1_RGBA_SRGB_BLOCK`
  (block size stays 8 → identical mip math); the only behavioural delta is the
  3-color-mode index-3 texel now reads `.a==0`, which is exactly the punch-through
  bit the alpha-test discard needs. `format_has_alpha` correctly still excludes
  BC1 (a 1-bit mask must not be read as a gloss gradient). **0 findings.**
- **BSGeometry mesh extraction** — the coord SoT refactor (`d9f2dbb8`), bitangent
  sign fix (`df60da80`), SSE-recon edits (`5b05b3e9`/`b1c942fc`), and tint/inner-
  layer forwarding (`1605be44`) were each confirmed to either not touch or
  additively-and-correctly extend the BSGeometry path. No mirror/inversion, no
  wrong-handed tangent, no double-swap. **0 findings.**
- **CDB materials** — the `73be72d9` dead-map removal is functionally inert; the
  CDB parse path is byte-for-byte unchanged. Known #1569/#1571 unchanged. **0 new.**
- **NIF shader 155+** — #1606 (SF LOD `BSLightingShaderProperty` +38 B under-read)
  confirmed still open; **severity stays MEDIUM** — the per-block `block_sizes`
  realignment (`lib.rs:476`) snaps the stream to the declared block end, so the
  under-read is contained to the single LOD block and does **not** desync following
  blocks. **0 new.**
- **ESM + cell bring-up** — the `234c6f1a` XCLL reclassification touches only the
  FO4/FO76 buckets; `XCLL_SIZES_STARFIELD = [28, 108]` and the
  `game==Starfield && len>=108` decode branch are byte-identical pre/post commit
  (a separate `#1579` already widened the gate from `==108` to `>=108`, tested). All
  seven SF spawn/cell regression guards (`base_layer`-gated collider, `SceneFlags`,
  `DoorTeleport`, `FormIdComponent`/`LocalBound`/`BSXFlags`, `SkinSlotPool`) intact.
  The only ESM-dispatch change (`457db492` SCOL for FNV/FO3) leaves SF unchanged.
  **0 new.**
- **NIFAL material translation** — `translate_material` remains the single boundary;
  metalness/roughness still plain resolved `f32`; the relocated `include/pbr.glsl` +
  `include/material_sampling.glsl` carry only parameterized BRDF math (no per-draw
  keyword classification leaked in). The GLSL `GpuMaterial` (now single-sourced in
  `include/bindings.glsl`) matches the Rust `#[repr(C)]` struct field-for-field;
  size pin (300 B) and offset test both green. **2 NEW LOW/INFO** (see below).
- **BGSM/BGEM external flow** — the two FO4 precombine commits (`efd3c41b`,
  `022cac83`) add only FO4-CSG-path call sites and a CSG-decode `is_decal` field;
  neither modifies the shared `merge_bgsm_into_mesh` / `pack_bgsm_material_flags`,
  so the SF BSGeometry+BGSM handoff is unaffected. #1580 (BGEM
  `grayscale_to_palette_alpha` not forwarded) confirmed still open. **0 new.**

## Findings by Severity

| Dim | Area | CRITICAL | HIGH | MEDIUM | LOW | INFO/disproof |
|-----|------|---------:|-----:|-------:|----:|--------------:|
| 1 | BA2 v2/v3 LZ4 | 0 | 0 | 0 | 0 | regression re-verified |
| 1b | DDS BC1 alpha (`2aac5351`) | 0 | 0 | 0 | 0 | new commit cleared |
| 2 | BSGeometry mesh | 0 | 0 | 0 | 0 | 4 commits cleared |
| 3 | CDB materials | 0 | 0 | 0 | 0 | dead-map removal inert |
| 4/5 | ESM + cell bring-up | 0 | 0 | 0 | 0 | XCLL split SF-safe |
| 6 | NIF shader 155+ | 0 | 0 | 0 | 0 | #1606 stays MEDIUM |
| 7 | Real-data validation | 0 | 0 | 0 | 0 | paths unchanged |
| 8 | NIFAL translation | 0 | 0 | 0 | 1 | 1 INFO |
| 9 | BGSM/BGEM flow | 0 | 0 | 0 | 0 | precombine commits FO4-only |
| **Total** | | **0** | **0** | **0** | **1** | 1 INFO |

**Counts (actionable): CRITICAL=0 HIGH=0 MEDIUM=0 LOW=1 (+1 INFO) TOTAL=1.**

---

## LOW

### SF-D8-01: GLSL `GpuMaterial` field order has no automated cross-check against the Rust `#[repr(C)]` struct
- **Severity**: LOW (hardening; no current drift)
- **Dimension**: NIFAL canonical material translation
- **Location**: `crates/renderer/src/vulkan/material.rs:1340` (offset test, Rust-only) vs `crates/renderer/shaders/include/bindings.glsl:61` (sole GLSL declaration); GLSL-parsing test at `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:352`
- **Status**: NEW (latent gap; not a regression — the GLSL `GpuMaterial` copy has never had a positive field-order guard, the `218b425b` split merely consolidated it into one lintable file)
- **Description**: The `gpu_material_field_offsets_match_shader_contract` / `gpu_material_size_is_300_bytes` tests pin the Rust layout using `offset_of!` / `size_of` only — they are self-referential and never parse the GLSL `struct GpuMaterial`. The companion GLSL-parsing tests in `gpu_instance_layout_tests.rs` walk shader source but assert only the *absence* of a `struct GpuMaterial` declaration in `ui.vert` / `water.vert` (the single-source-of-truth guard) and positively cross-check **`GpuInstance`** field order, not `GpuMaterial`. So a future edit that reorders a field inside `bindings.glsl`'s `GpuMaterial` (e.g. swapping `metalness`/`roughness`, or any within-vec4 reorder that keeps the 300 B size) would compile, pass every `cargo test`, and silently produce wrong shader reads on every lit surface.
- **Evidence**: `grep "struct GpuMaterial"` → one GLSL hit (`bindings.glsl:61`) + the Rust struct (`material.rs:69`). `gpu_instance_layout_tests.rs:352-388` only asserts `!src.contains("struct GpuMaterial")` for `ui.vert`/`water.vert`; no test extracts the `GpuMaterial { ... }` body and compares the ordered field-name list to the Rust struct. The field-for-field match was verified by hand this audit (16 vec4 groups, identical order/type), so the gap is latent.
- **Impact**: A within-struct reorder of the now-single-sourced GLSL `GpuMaterial` would not be caught at `cargo test` and would corrupt every lit-surface read — the exact failure class the severity rubric rates HIGH *if it ships*. Zero impact today (no drift), all-game blast radius if it ever regresses. Reported LOW because the latent guard gap, not an active defect.
- **Related**: SF-D8-02 (stale doc references in the same files); the `GpuInstance` GLSL-parsing guard that already exists is the template to copy.
- **Suggested Fix**: Add a GLSL-parsing test mirroring the existing `GpuInstance` one: `include_str!("../../../shaders/include/bindings.glsl")`, extract the `struct GpuMaterial { ... }` block, and assert its ordered field-name list matches the Rust struct's field order. Cheap, runs without glslang, closes the only un-guarded leg of the GpuMaterial lockstep contract.

---

## INFO / Verification (no action; recorded for the trail)

### SF-D8-02: Stale `triangle.frag` source-of-truth references in GpuMaterial sync doc comments
- **Severity**: INFO (doc rot, post-`218b425b` shader split)
- **Dimension**: NIFAL
- **Location**: `crates/renderer/src/vulkan/material.rs:56-57`, `:1180`, `:1224`, `:1340` ("`struct GpuMaterial` declaration at `triangle.frag:110-184`"); the module-doc at `material.rs:21-25` ("only `triangle.frag` mirrors GpuMaterial"); `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` GpuMaterial-absence assertion messages still phrase the contract as "only `triangle.frag` mirrors the material struct"
- **Status**: NEW
- **Description**: The `218b425b` split moved the `struct GpuMaterial` declaration out of `triangle.frag` and into `crates/renderer/shaders/include/bindings.glsl:61`; `triangle.frag` now only `#include`s it. Several Rust doc comments and test-message strings still name `triangle.frag:110-184` / "only triangle.frag mirrors GpuMaterial" as the authoritative shader-side declaration site.
- **Evidence**: `git show 218b425b` moved the struct into `bindings.glsl` (+309 lines); `grep "struct GpuMaterial" crates/renderer/shaders/triangle.frag` → zero hits; the only GLSL declaration is `bindings.glsl:61`.
- **Impact**: None at runtime; misleads the next person editing the material contract toward a file that no longer holds the declaration.
- **Suggested Fix**: Update the comments/messages to point at `crates/renderer/shaders/include/bindings.glsl` as the single GLSL declaration site (fold into the SF-D8-01 fix).

---

## Disproof Log (regressions investigated and cleared)

These are not findings — they are the disproved hypotheses that justify the
"clean" verdict on each recently-changed path. Recorded so the next audit can skip
re-deriving them.

- **BA2 LZ4 undersized-`max_size` heap overflow** — re-disproved. `lz4_flex::block::decompress(packed, unpacked_size)` (`ba2.rs:694`) size-checks against the declared `unpacked_size` and errors on mismatch; size fields are pre-capped via `checked_chunk_size`/`checked_entry_count` before any `vec![0u8; n]`. Hard bound, not a hint.
- **BC1 punch-through alpha breaking opaque meshes / mip math** — disproved. Block size stays 8 (identical `mip_size`/staging/`BufferImageCopy` math); opaque meshes ignore `.a` (shader discard gated on `INSTANCE_FLAG_ALPHA_BLEND` / CPU-classified `MATERIAL_KIND_GLASS`), so stray 3-color-mode transparent texels are harmless. `BC1_RGB_SRGB_BLOCK` is now unreachable from production parsing (only in `average_rgb` back-compat + one direct-struct test).
- **Coord SoT refactor (`d9f2dbb8`) mirroring/inverting SF geometry** — disproved. BSGeometry vertices/normals/tangents are already Y-up (decoded by `unpack_udec3_xyzw`); the swap edits in `tangent.rs` replace inline `[x,z,-y]` with `coord::zup_to_yup_pos` (bit-identical `(x,z,-y)`); BSGeometry's Y-up fallback uses `synthesize_tangents_yup` (no swap, untouched). No double-swap, no missed swap.
- **Bitangent sign fix (`df60da80`) wrong-handed on SF** — disproved. SF authored tangents take the bitangent sign from the UDEC3 W channel (never calls `bitangent_sign`); the SF fallback calls `bitangent_sign(n, t=∂P/∂U, b=∂P/∂V)` with correct operand order, yielding +1 for the Y-up right-handed case.
- **SSE-recon edits (`5b05b3e9`/`b1c942fc`) reaching SF** — disproved. Both live entirely inside `decode_sse_packed_buffer` (Skyrim-SE BSTriShape skin reconstruction); BSGeometry never invokes it and `unpack_udec3_xyzw` was not touched.
- **tint_map/inner_layer_map forwarding (`1605be44`) clobbering SF material handoff** — disproved. Purely additive; `merge_bgsm_into_mesh` has no reference to either field, so a BGSM merge on an SF BSGeometry mesh leaves the NIF-captured values intact.
- **sfmaterial dead-map removal (`73be72d9`) altering CDB parse** — disproved. Only the write-only `class_by_type_id` field/init/insert were removed; `class.type_id` is still parsed and read elsewhere; `parse`/`index_chunks`/`parse_class` byte-for-byte unchanged.
- **XCLL reclassification (`234c6f1a`) regressing SF** — disproved. Diff touches only the FO4/FO76 buckets; `XCLL_SIZES_STARFIELD = [28, 108]` and the SF 108-byte decode branch are byte-identical pre/post; the 21 XCLL gate tests pass.
- **#1606 SF LOD shader under-read desyncing following blocks (escalate to HIGH?)** — disproved. The per-block `block_sizes` realignment `stream.set_position(start_pos + size)` (`lib.rs:476`) snaps the stream to the declared block end after every block; the +38 B under-read is recorded in `drift_histogram` and contained to the single LOD `BSLightingShaderProperty` block. Stays MEDIUM.
- **FO4 precombine commits (`efd3c41b`/`022cac83`) breaking shared SF BGSM merge** — disproved. They add only FO4-CSG-path call sites and a CSG-decode `is_decal` field; the shared `merge_bgsm_into_mesh` / `pack_bgsm_material_flags` are untouched and game-agnostic.

## CRC32 Flag Table

Unchanged from 2026-06-14. The maintained name table at
`crates/nif/src/shader_flags.rs` (`bs_shader_crc32`, pinned to nif.xml literals)
maps known CRC32 hashes → flag names for the `BSLightingShaderProperty` /
`BSEffectShaderProperty` SF1/SF2 arrays; gates `FO4_CRC_FLAGS = 132` (SF1) and
`FO76_SF2_CRCS = 152` (SF2) in `crates/nif/src/version.rs`. No new derivation this
audit.

## Remaining-Work Chain (per `starfield-esm-roadmap.md`)

Unchanged from 2026-06-14 — Phases 0+1 done, Phases 2-4 invalidated by the 99.9%
parity measurement. In priority order (both the BGSM parser and the ESM parser
have shipped):

1. **Per-field CDB extraction** (#1290 / #1289 Phase 2). `.mat`-resolved materials
   still reach the Disney lobe with NIF-keyword-guessed PBR; the authored CDB
   dataset is parsed and discarded. Top renderable-fidelity blocker. Fold DLC CDB
   paths (#1571) and per-instance recovery (#1569) into the same change.
2. **ESM-placed LIGH decode** (#1567). 656 Cydonia lights dropped on the `BFCB`
   component-block layout; reuse the same `BFCB` walker for model-less STAT/ACTI/
   ARMO (#1576).
3. **PDCL + decal-projection** (#1568). 1 846 placed decals; needs a projection
   system — defer, add a warned-skip arm in the interim.
4. **StarfieldLighting forwarding** (#1578) — gravity_scale + height-fog decoded
   but stop at the runtime resource boundary; wire when a consumer lands.
5. **SF LOD shader under-read tail** (#1606) — contained MEDIUM; byte-audit the
   LOD `BSLightingShaderProperty` variant tail against nif.xml.
6. **Exterior worldspace tiles / space-cell / planet / GBFM records** — deferred.

## What's Possible Today (carried forward; paths unchanged this round)

- **Walkable Cydonia interior** — `--esm Starfield.esm --sf-smoke citycydoniamainlevel`
  resolves ~88.8% of REFRs to base statics (no decode path changed since the
  baseline, so no regression expected). Gaps: ESM-placed lights (#1567), decals
  (#1568).
- **Individual mesh + texture visualization** — geometry + textures resolve;
  Disney BSDF runs on keyword-guessed PBR for `.mat` content until #1289 Phase 2.
- **BA2 v2/v3 extraction** — 129/129 archives, 0 failures (re-verified inert).

## References

- Dedup baseline: `/tmp/audit/issues.json` (29 open issues)
- Prior audit: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (its HIGH/MEDIUM findings
  are now the filed open issues #1567/#1568/#1569/#1571/#1576/#1578/#1580/#1606 and
  #1290/#1289-Phase2 — this pass confirmed those remain the complete gap set and no
  regression was introduced by the commit wave since)

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-06-18.md` (will file 1 LOW
hardening finding SF-D8-01; SF-D8-02 is INFO doc-rot, fold into the same fix).
