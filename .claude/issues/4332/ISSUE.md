# #4332 SCR-D8-2026-09-14-01: the HKX packfile gate ignores fileVersion / contentsVersion / reusePaddingOptimization, so a non-hk_2010 layout is misread rather than refused

**Labels**: low,scripting,animation,safety,bug,game:skyrim
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: LOW
- **Dimension**: Havok Idle / Cinematic Slice
- **Untrusted-Input**: Yes (no panic; wrong decode or a misleading error)
- **Location**: `crates/hkx/src/packfile.rs::Packfile::parse` (layout-rules check); `crates/hkx/src/animation.rs::Layout::new`
- **Status**: NEW (made broader by `fb8173fe`, which made the header the only source of field offsets)
- **Description**: `parse` validates `layoutRules[0]` (pointer size) and `[1]` (endianness) only. `Layout::new` assumes the Havok 2010 class field order (`m_fileVersion` @0xC and `m_contentsVersion` @0x28 are never read) and MSVC layout with `reusePaddingOptimization == 0` (@0x12 never read). Per `hkStructureLayout.h`, GCC-style tail-padding reuse shifts derived members by 4 on 64-bit. FO4 `hk_2014.1.0-r1` files (fileVersion 11) are refused only by accident, with the misleading error `global fixups precede local fixups` (0 of 404 decode).
- **Evidence**: Dim 8 probe: every sampled vanilla LE/SE file reads fileVersion 8, `hk_2010.2.0-r1`, `[4|8,1,0,1]`.
- **Impact**: No impact on vanilla content. A modded Skyrim `.hkx` in another Havok layout would animate wrong instead of logging "unsupported layout". The consumer is Skyrim-gated.
- **Related**: #3011
- **Suggested Fix**: Return `UnsupportedLayout` unless `fileVersion == 8`, `layoutRules[2] == 0` and `contentsVersion` starts with `hk_2010`. Add rejection cases next to `rejects_layouts_other_than_32_or_64_bit_little_endian`, plus a CI-running `Layout::new(4)` offset pin.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
