# REN-D5-2026-09-20-01: DDS untrusted-input path: mip_size is unchecked u32 arithmetic, parse_dds has no dimension cap, and the payload check is a release assert! — a malformed archive texture panics the engine or wraps to a 0-byte budget under a 4 GiB image

- **ID**: REN-D5-2026-09-20-01
- **Labels**: high,renderer,memory,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: HIGH · **Dimension**: Memory/Lifecycle
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D5-2026-09-20-01)

**Location**: `crates/renderer/src/vulkan/dds.rs:537-547` (`mip_size`), `:259-330` (`parse_dds`), `:276` (unclamped mip_count); `crates/renderer/src/vulkan/texture.rs:318-328` (`assert!` payload check)

**Description**
Three coupled gaps on the archive-DDS path (mod-authorable input): (1) a truncated-but-plausible DDS panics in release via the `assert!`; (2) in release `mip_size` wraps — at the NVIDIA 32768² limit, 32768×32768×4 = 2^32 wraps to 0, so total_data_size returns 0, a 128-byte header-only file passes, and vkCreateImage allocates a 4 GiB device-local image sized from on-disk fields with no cap; (3) debug builds panic earlier on multiply/shift overflow (mip_count ≥ 33).

**Evidence**
`mip_size` is plain u32 multiply; width/height read raw; `assert!(pixel_data.len() as u64 >= total_size, …)` compiles into release. In-repo fix precedent: `crates/menuxml/src/tex.rs:88` (8192 cap + `TexError::AbsurdDimensions`) — the renderer's decoder is the only DDS entry point without it. No OOB read occurs (wrapped sizes bound the reads): a VRAM/OOM bomb and a panic, not corruption.

**Impact**
A hostile or corrupt mod archive crashes the engine or exhausts VRAM for the session; vanilla content never triggers it.

**Suggested Fix**
Cap width/height in parse_dds (menuxml's 8192 bound or device maxImageDimension2D), clamp mip_count to log2(max(w,h))+1, compute mip_size in u64/checked_mul, convert the assert! to an anyhow error so the caller falls back to the checkerboard; add the absurd-dimension test the audit skill asks for.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
