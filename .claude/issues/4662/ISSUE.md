# PAR-D2-2026-09-21-02: BA2 short-decode warnings name neither the archive nor the entry, and DX10 never checks a chunk's decoded length

Labels: low,bug,import-pipeline,game:starfield

## Description
`crates/bsa/src/ba2.rs:762-766` (zlib arm), `:836-841` / `:848-866` (LZ4 arm) in `decompress_chunk` all warn with no path in scope — `extract_general` and `extract_dx10` (`:880-913`) don't receive or pass the entry path through, unlike the BSA sibling (`archive/extract.rs:173-183`), which does name the path.

`extract_dx10` concatenates each chunk's actual decoded length with no check against `chunk.unpacked_size`. A short non-final chunk shifts every later mip under the unchanged synthesized header. CSG rejects the equivalent case (#1986); BA2 DX10 does not.

Verified unchanged at HEAD `ee6d3fb39`: `decompress_chunk`'s warnings are still path-less, and `extract_dx10`'s per-chunk loop still has no length check against `chunk.unpacked_size`.

## Evidence
See the cited locations (both `decompress_chunk`'s two warn! call sites and `extract_dx10`'s chunk-concatenation loop).

## Impact
Downgraded from MEDIUM after reading the consumer: `crates/renderer/src/vulkan/texture.rs:339-350` (#4511) rejects a payload shorter than the mip chain, so the usual outcome is a checkerboard plus a renderer-side "DDS pixel data too small" error that does not point back at the BA2. A silent mip shift remains possible only when another chunk over-delivers to compensate. LZ4 is the only codec for every Starfield v3 texture archive, which is where the DX10 chunk-concatenation path runs unconditionally.

## Related
#2618 (closed — added the warning this finding says still lacks a path), #812, #1986 (the CSG-side equivalent check), #4511 (the renderer-side guard that currently limits the blast radius)

## Suggested Fix
Thread the entry path into `extract_general`/`extract_dx10`/`decompress_chunk` for the warning. For DX10, reject (or zero-pad to `unpacked_size`) a short non-final chunk, after measuring vanilla with the `ba2_real` sweep.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D2-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix