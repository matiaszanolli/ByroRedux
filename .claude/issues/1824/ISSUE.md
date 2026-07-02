# 1824: FO4-D2-02: gl_to_gamebryo_blend truncates u32 src/dst via 'as u8' with no range guard

URL: https://github.com/matiaszanolli/ByroRedux/issues/1824
Labels: bug, renderer, low, legacy-compat

**Severity**: LOW
**Dimension**: 2 — BGSM/BGEM Consumption
**Location**: `byroredux/src/asset_provider/material.rs:497-503`
**Status**: NEW
**Related**: FO4-D2-01 (same function, filed separately as the correctness regression)

## Description

`src_blend`/`dst_blend` are parsed as `u32` (`crates/bgsm/src/base.rs:51-52`) and
passed to `gl_to_gamebryo_blend(gl: u32) -> u8`, whose fallthrough does `other as
u8`. A malformed/modded factor ≥ 256 wraps silently (`256 → 0`), and values in
`11..=255` land in the renderer's `_ => SRC_ALPHA` catch-all
(`crates/renderer/src/vulkan/pipeline.rs:175`) with no diagnostic. Not a
vanilla-content problem (authored values are 0/1/4/6/7 per the reference parser);
a latent footgun for corrupt/modded input. Whatever the correctness fix for
FO4-D2-01 lands as, this guard gap should be addressed in the same function.

## Evidence

- `material.rs:501` — `other => other as u8`.
- Renderer catch-all — `pipeline.rs:175` — `_ => SRC_ALPHA`.

## Impact

Corrupt/out-of-spec modded blend factors map to an arbitrary `vk::BlendFactor`
with no log. Cosmetic-only, vanilla-safe.

## Suggested Fix

Clamp/validate to the known `0..=10` domain and `log::warn!` once on
out-of-range input, mirroring the magic-vs-extension warn already used
elsewhere in the same file.

## Completeness Checks
- [ ] **SIBLING**: Apply the same guard to both the BGSM and BGEM call sites of `gl_to_gamebryo_blend`.
- [ ] **TESTS**: A regression test pins the out-of-range (≥11, ≥256) behavior to a clamp + warn instead of silent wraparound.

