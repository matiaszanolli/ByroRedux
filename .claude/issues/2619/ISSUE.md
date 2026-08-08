# SF-D1-03: renderer map_dxgi_format has no arm for DXGI 10/11/31, 78 Starfield textures fall back to placeholder

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2619
**Finding ID**: SF-D1-03

**Severity**: MEDIUM
**Dimension**: 1 (BA2 v2/v3 LZ4 Block Decompression)
**Location**: `crates/renderer/src/vulkan/dds.rs:508-552` (`map_dxgi_format`)
**Status**: NEW

## Description
The same 78 records SF-D1-02 identifies (missing `pitch_or_linear_size_for`
arms for DXGI 10/11/31) also hard-fail the renderer's `map_dxgi_format` —
every Starfield interior cubemap and chargen face normal map falls back to
the placeholder texture. BA2 extraction of these 78 textures is byte-exact
correct; the renderer's DXGI table simply has no arm for 10/11/31 and bails
at parse time.

## Evidence
The same 78-record set as SF-D1-02 — 12 interior ambient/reflection-probe
cubemaps (`cell_cavecube`, `cell_shipinteriorcube`, …) + the LTC LUT + 62
chargen head normal maps + 2 gas-giant gradients.

## Impact
Missing textures, not a crash — but per the project's own
"chrome/posterized ⇒ missing textures" diagnosis rule (see
[[feedback_chrome_means_missing_textures]]), this is exactly the defect
class that costs hours downstream, concentrated on interior ambient
lighting and every chargen face.

## Suggested Fix
Add core-Vulkan-1.0 format arms for DXGI 10/11/31 with matching tests.

## Related
SF-D1-02 (BA2 side of the same 78 records).

## Completeness Checks
- [ ] **SIBLING**: Fix alongside SF-D1-02 — same 78-record set, two independent gaps
- [ ] **TESTS**: A fixture for each of DXGI 10/11/31 asserts a valid Vulkan format is returned
