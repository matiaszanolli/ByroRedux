# ESM-5-03: `LightData.near_clip` (DATA byte 24) is labelled the god-ray near clip, but it is the shadow-bias Near Clip; the actual god-ray near clip (FO4 DATA byte 52) is not decoded

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6795
Labels: low, esm-plugin, bug, game:fo4
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (decoded for the census, "consumed by nothing", PRD Q8)
- **Dimension**: Base-object records read through the cell walker (LIGH)
- **Record / Sub-record**: FO4 `LIGH DATA`
- **Location**:
  - `crates/plugin/src/esm/cell/base_objects.rs:79-86` (the doc comment)
  - `crates/plugin/src/esm/cell/support.rs:236` (`read_f32(24)`)
  - Test `parse_ligh_wgdr_and_near_clip` in `cell/tests/light.rs`
- **Status**: NEW
- **Description**:
  - The FO4 xEdit `LIGH DATA` is 64 bytes: Time 0, Radius 4, Color 8, Flags 12, Falloff 16, FOV 20, Near Clip 24, Flicker 28-39, Constant 40, Scalar 44, Exponent 48, **`God Rays - Near Clip` 52**, Value 56, Weight 60. Skyrim has Near Clip at 24 only.
  - The commit and the field doc say byte 24 is "the Near Clip the CK ties to the god-ray profile".
  - The FO4 CK wiki (`falloutck-uesp-wiki/(main)/Light.wiki:27,60`) separates two controls. Near Clip (shadow bias, default 7.2174) is the general one. The "God Rays: Near Clip" control sits under the God Rays group.
- **Evidence** (Fallout4.esm LIGH, 64-byte DATA):
  - Byte 24: 7.217 on 261 records, the wiki's shadow-bias default. For the 20 lights with `WGDR`, byte 24 is 64.0 / 128.0 / 1.0.
  - Byte 52: 0.0 (308), 10.0 (209), 16.0 (2), 2.0 (1).
  - LIGH DATA sizes in FO4: 64 (520), 60 (174), 48 (107), so the 52 field exists only on the 64-byte form.
- **Impact**: None today. A future VLM god-ray consumer that reads `near_clip` gets the shadow-bias value (7.2 or 64, 128 for `WGDR` lights) instead of 0 / 10 / 16.
- **Suggested fix**: Rename or re-document `near_clip` as the shadow Near Clip (Skyrim+, byte 24). Add `god_rays_near_clip: Option<f32>` read from byte 52 when `len >= 56`. Correct the VLM-HF-011 PRD wording.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
