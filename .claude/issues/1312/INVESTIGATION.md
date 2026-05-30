# Investigation — #1312 (OBL-D3-02) Oblivion 36-byte XCLL drops dir_fade + fog_clip

## Premise — CONFIRMED, and the data loss is total for Oblivion
The XCLL decode in `cell/walkers.rs` gated the extended fields on a single
`sub.data.len() >= 40`. But the FULL Oblivion (TES4) XCLL is **36 bytes**, so that gate
was *never* satisfied by vanilla Oblivion content → `directional_fade`(@28) and
`fog_clip`(@32) were silently `None` on **every** Oblivion cell, even though both are
authored. The size table `XCLL_SIZES_OBLIVION = [28, 32, 36]` accepted 36 with no warn,
masking the loss (same defect class as the Starfield #1291 size bug).

### Authoritative layout (xEdit TES4 `wbStruct(XCLL)` + OpenMW `loadcell.cpp`)
```
 0 Ambient / 4 Directional / 8 Fog Color (RGBA)   ┐
12 Fog Near / 16 Fog Far                           │ shared (0-27)
20 Directional Rotation XY / 24 …Z (S32)           ┘
28 Directional Fade (f32)
32 Fog Clip Dist  (f32)
= 36  (TES4 has NO Fog Power — that is the FO3/FNV 40-byte addition)
```
OpenMW `loadcell.cpp`: `case 36: // TES4 reader.get(&mLighting, 36)`; `case 40: // FO3/FNV`.

## Fix — per-field gating (more correct than the issue's suggestion)
The issue suggested "dir_fade + fog_clip at `>= 36`", but that would still drop
`dir_fade` for a 32-byte XCLL (also a valid TES4 size). The fields are independently
sized, so each is gated on its own offset:
```rust
let dir_fade  = (len >= 32).then(|| r.f32_or_default()); // @28
let fog_clip  = (len >= 36).then(|| r.f32_or_default()); // @32
let fog_power = (len >= 40).then(|| r.f32_or_default()); // @36 (FO3/FNV only)
```
`.then(|| …)` only advances the reader when the field is present, so the cursor stays
aligned at 28 / 32 / 36 / 40. Also corrected the `walkers.rs:15-18` size doc, which
wrongly called the 32/36 tails "padding with no extended fields."

## Verification (real `Oblivion.esm`, 1855 interior cells, 1770 with XCLL)
- `directional_fade` populated: **0 → 1770** (all)
- `fog_clip` populated: **0 → 1770** (all)
- `fog_power` populated: **0** (correct — TES4 has none)

## Completeness checks
- **SIBLING (FO3/FNV)**: the decode is the shared path. FO3/FNV ship 40-byte XCLLs →
  per-field gating reads all 3 (dir_fade@>=32, fog_clip@>=36, fog_power@>=40), identical
  to the old `>= 40` behaviour. Existing `parse_cell_fnv_xcll_extracts_40byte_tail…`
  test stays green. Skyrim 92 / Starfield 108 paths unaffected (SF takes its own branch).
- **TESTS**: added `parse_cell_oblivion_36byte_xcll_extracts_dir_fade_and_fog_clip` and
  `…_32byte_xcll_extracts_dir_fade_only` (+ a shared `parse_oblivion_xcll` helper).
  Plugin lib 447 → 449.
- **CANONICAL-BOUNDARY / UNSAFE**: N/A (ESM parse-side; no unsafe).

## Sources
xEdit `Core/wbDefinitionsTES4.pas` `wbStruct(XCLL,'Lighting')` (branch xedit-4.1.5p);
OpenMW `components/esm4/loadcell.cpp` XCLL size switch.
