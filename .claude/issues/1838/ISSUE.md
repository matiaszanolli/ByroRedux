# NIF-D2-01: #1277 refactor reverted the #982 fix — variant-helper gates re-introduced at both tri_shape sites, with the contradicting comments still in place

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1838
**Labels**: bug, nif-parser, medium

**Severity**: MEDIUM
**Dimension**: Version Gating
**Location**: `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:135` (`has_shader_alpha_refs()`), `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:341` (`has_material_crc()`)
**Regression of**: #982 (closed 2026-05-12; audit finding NIF-D2-NEW-07, 2026-05-12)

## Description

**Game Affected**: no retail title (all seven shipping games classify onto the aligned side); divergence hits unclassified/hybrid headers only — `(uv=11, bsver 35..=82)` → `Unknown` → helper returns false while nif.xml `#BS_GT_FO3#` (`bsver > 34`) authors the fields (8-byte under-read of shader/alpha refs, 4-byte under-read of material CRC); `(uv=12, bsver ≤ 34)` → `SkyrimLE` → helper true where the fields are absent (over-read); `(uv ≤ 10, bsver > 34)` → `Oblivion` → false (under-read).

Commit `01957517` (Fix #982) deliberately replaced `variant().has_shader_alpha_refs()` / `variant().has_material_crc()` at these two sites with raw `stream.bsver() > 34`, adding comments explaining the BSVER 35..=82 `Unknown`-corner false-negative. Commit `2bd447d5` (refactor, #1277 Task 5) migrated both sites **back** to the variant helpers — listing them as "existing helpers now called from production" — while leaving the raw-bsver-mandating comments untouched. The commit applied the Unknown-semantics analysis to five *other* sites ("KEPT on raw bsver") but missed that these two carried the identical constraint from a shipped audit fix.

## Evidence

Current code at `ni_tri_shape.rs:128-135`:
```
// Query the file's actual bsver rather than the routed
// game variant — `variant().has_shader_alpha_refs()` returns
// false for BSVER in 35..=82 (the `Unknown` corner) even
// though nif.xml's `#BS_GT_FO3#` gate is `BSVER > 34` and
// the field IS authored there. Mirrors the
// `has_properties_list` site at `base.rs:103`. See
// NIF-D2-NEW-07 (audit 2026-05-12).
if stream.variant().has_shader_alpha_refs() {
```
Same contradiction at `:337-341` for `has_material_crc()`. `git show 01957517` introduces the raw-bsver reads + comments; `git show 2bd447d5` flips both back. nif.xml gates purely on BSVER (`#BS_GT_FO3#`), never on user_version — raw bsver is the spec-correct read. Confirmed live against HEAD `1b4e8e84` during publish verification: both sites currently call the `variant()` helper with the contradicting raw-bsver comment still in place.

## Impact

Functional divergence confined to non-retail hybrid headers (the original #982 rated it "forward-compat hygiene"). Rated MEDIUM because (a) it silently undoes a shipped audit fix, (b) the surviving comments actively mislead the next maintainer, and (c) the 2026-06-23 audit certified this dimension clean over exactly this state.

## Suggested Fix

Restore `stream.bsver() > crate::version::bsver::FO3_FNV` at both sites (matching the comments), and add a regression test with a `(uv=11, bsver=50)` synthetic header so the next "migrate to helpers" sweep trips a red test instead of silently reverting again.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — NIF-D2-02 (#1839, `shape_compound.rs`/`node.rs`) and the four dead `NifVariant` helpers this fix would orphan (NIF-D2-03, #1840)
- [ ] **TESTS**: A regression test pins this specific fix (the `(uv=11, bsver=50)` synthetic header case, per the Suggested Fix)
