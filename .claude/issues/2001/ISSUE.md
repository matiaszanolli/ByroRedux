# 2001: NIF-D1-01: NiPersistentSrcTextureRendererData aliased to NiPixelData's parser — missing Pad Num Pixels + Platform fields

https://github.com/matiaszanolli/ByroRedux/issues/2001

Labels: high, nif-parser, nif, bug

**Severity**: HIGH on Oblivion path / MEDIUM on FO3+ · **Dimension**: Stream Position Integrity
**Location**: `crates/nif/src/blocks/mod.rs:604-606` (dispatch alias), `crates/nif/src/blocks/texture.rs:236-306` (`NiPixelData::parse`)
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIF_2026-07-16.md (NIF-D1-01)

## Description
`blocks/mod.rs` dispatches `"NiPixelData" | "NiPersistentSrcTextureRendererData"` to the same `NiPixelData::parse`, but the two types diverge after the shared `NiPixelFormat` prelude: `NiPersistentSrcTextureRendererData` additionally reads `Pad Num Pixels` (since 20.2.0.6) and an unconditional 4-byte `Platform` field that `NiPixelData` doesn't have.

## Evidence
```rust
// texture.rs:286-289 — correct only for NiPixelData
let num_pixels = stream.read_u32_le()? as usize;
let num_faces = stream.read_u32_le()?;   // actually "Pad Num Pixels" on FO3+ NiPersistentSrcTextureRendererData
let total_bytes = num_pixels * num_faces as usize;  // wrong length
let pixel_data = stream.read_bytes(total_bytes)?;   // Platform (4 B) never read
```

## Impact
Wherever this block type appears, the aliased parser misreads `Pad Num Pixels` as a byte-length multiplier and drops the mandatory `Platform` field. On Oblivion (no `block_sizes`) this cascades unrecoverably; on FO3+ it's masked by `block_sizes` reconciliation but leaves a corrupted `pixel_data` buffer silently in the scene.

## Related
None found in existing issues.

## Suggested Fix
Give `NiPersistentSrcTextureRendererData` its own parser (or a shared prelude + two-way tail split, matching the existing `NiPixelData` old/new-layout split), reading `Num Pixels` → `Pad Num Pixels` (since 20.2.0.6) → `Num Faces` → `Platform`/`Renderer` → `Pixel Data`.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (any other block type aliased to a parser it doesn't structurally match)
- [ ] TESTS: A regression test pins this specific fix (`NiPersistentSrcTextureRendererData` fixture with `Pad Num Pixels` + `Platform` present)
