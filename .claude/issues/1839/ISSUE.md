# NIF-D2-02: Same-commit helper substitutions at the MOPP Build-Type and BsMultiBoundNode culling-mode gates diverge from their nif.xml BSVER gates on hybrid headers

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1839
**Labels**: bug, nif-parser, low

**Severity**: LOW
**Dimension**: Version Gating
**Location**: `crates/nif/src/blocks/collision/shape_compound.rs:127` (`has_shader_alpha_refs()` gating `bhkMoppBvTreeShape.Build Type`), `crates/nif/src/blocks/node.rs:257` (`has_culling_mode()` gating `BSMultiBoundNode.Culling Mode`)
**Status**: NEW (introduced by `2bd447d5`, pre-baseline; never covered by #982, which fixed only the two `tri_shape` sites)

## Description

**Game Affected**: no retail title; hybrid tuples only — `(uv=11, bsver ≥ 35)` → `Unknown` → 1-byte under-read of MOPP `Build Type` / 4-byte under-read of `Culling Mode`; `(uv=12, bsver ≤ 34 / < 83)` → over-read

nif.xml gates `Build Type` on `#BS_GT_FO3#` (`bsver > 34`, nif.xml:3149) and `Culling Mode` on `#BS_GTE_SKY#` (`bsver >= 83`, nif.xml:6914) — pure BSVER predicates. Both sites were `stream.bsver()` compares until `2bd447d5` substituted variant helpers, inheriting the same Unknown/hybrid-corner divergence as NIF-D2-01 (#1838). Additionally the MOPP site answers a *collision-era* question with a *NiGeometry shader-refs* helper — the exact "can't tell the blessed family from the trap family" foot-gun #1511 documented.

## Evidence

`git log -L127,129:crates/nif/src/blocks/collision/shape_compound.rs` shows `- if stream.bsver() > 34` → `+ if stream.variant().has_shader_alpha_refs()` in `2bd447d5`; the in-code comment still says "Build Type: only for BSVER > 34". Confirmed live at HEAD: `shape_compound.rs:126-127` reads `// Build Type: only for BSVER > 34 (Skyrim+; FO3/FNV is 34)` immediately above `if stream.variant().has_shader_alpha_refs() {`; `node.rs:255-257` has the matching comment above `if stream.variant().has_culling_mode() {`.

## Impact

Zero on shipping content; doctrine/readability divergence and latent drift on unclassified exports (the class #160/#1331/#982 all chose raw bsver for).

## Suggested Fix

`stream.bsver() > crate::version::bsver::FO3_FNV` at `shape_compound.rs:127`; `stream.bsver() >= crate::version::bsver::SKYRIM_LE` at `node.rs:257`. Fixing this alongside NIF-D2-01 (#1838) fully orphans `has_shader_alpha_refs`, `has_material_crc`, `has_culling_mode` — fold into the NIF-D2-03 (#1840) decision.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked across related files — this finding IS the sibling sweep of NIF-D2-01 (#1838); coordinate the fix with that issue and with NIF-D2-03 (#1840, orphaned helper cleanup)
- [ ] **TESTS**: A regression test pins this specific fix (hybrid-header synthetic case for both the MOPP Build Type and BSMultiBoundNode Culling Mode gates)
