# FO3-D2-04: BSShaderNoLightingProperty falloff-absent default for falloff_start_angle disagrees with nif.xml (0.0 vs 1.0)

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2331

**Severity**: LOW
**Location**: `crates/nif/src/blocks/shader.rs:172-182`
**Status**: NEW

### Description
When `bsver <= 26`, the parser substitutes `(0.0, 0.0, 1.0, 0.0)` for the four falloff floats; nif.xml specifies defaults `(1.0, 0.0, 1.0, 0.0)` — `start_angle` is inverted.

Confirmed against current code and nif.xml: `blocks/shader.rs:180` fallback tuple is `(0.0, 0.0, 1.0, 0.0)`. nif.xml (`BSShaderNoLightingProperty`, "Falloff Start Angle" field) declares `default="1.0"` — the code's fallback for the first element is `0.0`, confirming the inversion.

### Impact
Unreachable on retail FO3/FNV (both ship bsver 34, always above the gate, so on-disk values are always read). Only reachable on transitional/dev/modded exports at bsver ≤ 26.

### Suggested Fix
Change the fallback tuple to `(1.0, 0.0, 1.0, 0.0)`.

### Related
#1331, #451

## Completeness Checks
- [ ] **TESTS**: A regression test pins "bsver <= 26 BSShaderNoLightingProperty falls back to falloff_start_angle == 1.0, matching nif.xml default"
