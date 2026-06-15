# #1595 — FO4-XCUT-MEDIUM-01: DXGI 88 (B8G8R8X8_UNORM) unhandled in renderer DDS loader -> textures render as missing-texture checker

**Severity**: MEDIUM · **Dimension**: BA2 Reader → Renderer (cross-cutting)
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-XCUT-MEDIUM-01)
**Location**: `crates/renderer/src/vulkan/dds.rs:205-242` (`map_dxgi_format`, no arm for 88 → `bail!("Unsupported DXGI format: 88")`)

## Description
`map_dxgi_format` has no arm for DXGI 88, so any format-88 DX10 texture fails the DDS load. The texture registry catches the `Err` and substitutes the magenta-checker fallback (verified `texture_registry.rs:109-119`), so it does not crash — but every format-88 texture renders as the missing-texture checker (the "Chrome → Missing Textures" symptom). DXGI 88 (B8G8R8X8_UNORM) is absent from vanilla base archives but present in `Unofficial Fallout 4 Patch - textures.ba2` (12×) and several popular mods (SS2Extended 1263×, settlement overhaul, America Rising 2, Institute Reformation, etc.) — 1,482 occurrences in the scanned corpus.

## Evidence
`dds.rs:241 _ => bail!("Unsupported DXGI format: {}", dxgi)`. `texture_registry.rs:109` documents the magenta-checker substitution on load failure. Corpus DXGI scan: `fmt88: 1482`.

## Impact
Vanilla base-game FO4 is unaffected (no format-88 textures). Modded installs that include UFO4P (near-ubiquitous) get checker-rendered surfaces wherever a format-88 texture is referenced — a visible content gap, but bounded to a single format with a one-line fix. MEDIUM because vanilla is clean and the fix is trivial.

## Related
FO4-D3-LOW-01 (the BA2-side sibling — pitch table); #1074 (added 56/61/87/91 to the renderer + BA2 tables — the analogous prior fix).

## Suggested Fix
`dds.rs:213` — add `DXGI_FORMAT_B8G8R8X8_UNORM (88) => Ok((vk::Format::B8G8R8A8_UNORM, 4, false))` (the X channel is "ignore"; B8G8R8A8 reads the same 4-byte BGRX layout, alpha sampled as 1.0). Pair with the FO4-D3-LOW-01 ba2.rs pitch-table fix and add a unit test for format 88.

## Completeness Checks
- [ ] **SIBLING**: BA2-side pitch table (FO4-D3-LOW-01) fixed in lockstep so both header synth and renderer agree on format 88
- [ ] **CANONICAL-BOUNDARY**: Format mapping stays in the DDS loader; no per-game branch in the shader
- [ ] **TESTS**: A unit test pins format 88 → `B8G8R8A8_UNORM` mapping
