# #1596 — FO4-D3-LOW-01: B8G8R8X8_UNORM (DXGI 88) absent from BA2 DDS pitch table

**Severity**: LOW · **Dimension**: BA2 Reader
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D3-LOW-01)
**Location**: `crates/bsa/src/ba2.rs:942-950` (`pitch_or_linear_size_for` bpp table)

## Description
`pitch_or_linear_size_for` lists uncompressed bpp only for DXGI 28/29/87/91 (4bpp), 56 (2bpp), 61 (1bpp). DXGI 88 = B8G8R8X8_UNORM (4bpp) is missing, so it falls into the unknown-format fallback which writes `total_bytes` with `DDSD_LINEARSIZE` instead of row pitch `width*4` with `DDSD_PITCH`. Formats 49/65 from the prior 2026-06-02 audit do NOT occur in the FO4 corpus and are excluded.

## Evidence
Corpus DXGI scan over every `*.ba2` DX10 archive in the FO4 Data dir: `fmt88: 1482` occurrences — present in `unofficial fallout 4 patch - textures.ba2` (12×), `ss2extended` (1263×), settlement overhaul, etc. Vanilla base `Textures1..9.ba2` carry only 71/74/77/83/87/61/98 (all handled). Format 49/65: zero anywhere. Current table: `28 | 29 | 87 | 91 => Some(4)` (`ba2.rs:943`).

## Impact
BA2-side only: the synthesized DDS header has a wrong `dwPitchOrLinearSize` + flag. The in-engine loader ignores this field (DX10 extended header disambiguates), so this header bug is export-tooling-only (texconv/DirectXTex). The in-engine content blocker is the sibling renderer gap — see FO4-XCUT-MEDIUM-01.

## Related
FO4-XCUT-MEDIUM-01 (renderer `map_dxgi_format` gap); #1074 (added 56/61/87/91 to the same tables).

## Suggested Fix
`ba2.rs:943` — add `88` to the 4-bpp arm: `28 | 29 | 87 | 88 | 91 => Some(4)`; add a unit test mirroring `pitch_bgra8_unorm_srgb_*` for format 88.

## Completeness Checks
- [ ] **SIBLING**: Renderer `map_dxgi_format` (FO4-XCUT-MEDIUM-01) fixed in lockstep
- [ ] **TESTS**: A unit test pins format 88 → `width*4` pitch with `DDSD_PITCH`
