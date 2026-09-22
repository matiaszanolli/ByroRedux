# PAR-D1-2026-09-21-03: BA2 DX10 header synthesis multiplies file-controlled dimensions in u32, panicking in debug builds

Labels: medium,bug,import-pipeline

## Description
`crates/bsa/src/ba2.rs:1070-1074` (`pitch_or_linear_size_for`) computes `bw * bh * bb` in `u32`, where `bw`/`bh` are the block-aligned width/height (`width.div_ceil(4).max(1)`) and `bb` is the per-block byte count for a 16-byte-block DXGI format (BC2/3/5/6H/7). With 16384 blocks on each side and 16 bytes per block, that product is exactly 2^32 and overflows. The function runs at extract time (`extract_dx10` -> `build_dds_header`), not at open time, so a crafted record is only caught when someone actually extracts it.

Verified unchanged at HEAD `ee6d3fb39`: the multiplication is still plain `u32` arithmetic with no widening or saturation.

## Evidence
Probe `ba2-dx10` (a one-chunk synthetic v1 DX10 BA2):

```
65535x65535 BC7 -> panicked at crates/bsa/src/ba2.rs:1073:17: attempt to multiply with overflow
65533x65533 BC7 -> same panic
65532x65532 BC7 -> Ok, dwPitchOrLinearSize = 4,294,443,024
65535x65535 BC1 -> Ok, 2,147,483,648
```

## Impact
Debug builds (`cargo run`) panic on the main thread (`resolve_texture` -> `TextureProvider::extract`, no `catch_unwind`) from a crafted or corrupt mod BA2. Release builds wrap silently — the damage there is header-only, because `crates/renderer/src/vulkan/dds.rs` recomputes mip sizes and ignores this field. Precedent #4155 (a debug-only `u32` overflow panic in a Havok reader) was rated MEDIUM.

## Related
#4155 (same debug-overflow class), #594, #2628

## Suggested Fix
Compute in `u64` and saturate to `u32::MAX` (or reject absurd dimensions at record-read time with a named error). Add the 65535^2 BC7 case as a unit test.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix