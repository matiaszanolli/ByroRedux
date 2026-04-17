# OBL-D3-C2: LIGH DATA color bytes read as RGB but stored BGRA (cross-game)

**Issue**: #389 — https://github.com/matiaszanolli/ByroRedux/issues/389
**Labels**: bug, renderer, critical, legacy-compat

---

## Finding

`crates/plugin/src/esm/cell.rs:854-856` reads LIGH DATA color bytes 8-10 as `(R, G, B)`:

```rust
let r = sub.data[8] as f32 / 255.0;
let g = sub.data[9] as f32 / 255.0;
let b = sub.data[10] as f32 / 255.0;
```

The on-disk layout is actually BGRA (verified via Gamebryo 2.3 source + UESP TES4:Fields#LIGH). Byte-level sample from real Oblivion:

```
LIGH 'RootGreenBright0650' DATA[32] = [FF FF FF FF, 8A 02 00 00, 36 74 66 00, 00 04 00 00, ...]
                                       time=-1      radius=650    B=36 G=74 R=66 pad   flags
```

The light is authored green (`G=0x74 = 116`) with lesser blue (`B=0x36 = 54`) and red (`R=0x66 = 102`) channels. Our parser assigns `r=0x36` (the B channel) and `b=0x66` (the R channel), so green lights render as magenta-ish and every torch (BGRA warm orange) renders cyan.

## Scope — cross-game

This is NOT Oblivion-specific. FNV LIGH DATA is also BGRA per the ESM format spec. Every torch/brazier/candle placed via a LIGH record has been color-swapped on every supported game since the LIGH parser was first added.

## Fix (3 lines)

```rust
let b = sub.data[8]  as f32 / 255.0;
let g = sub.data[9]  as f32 / 255.0;
let r = sub.data[10] as f32 / 255.0;
```

Verify against `DefaultTorch01` (expected warm orange) and a known green Ayleid well light.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check any other DATA parse path that reads color bytes from ESM records (RCLR for REGN, possibly worldspace XCLR).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Unit test with synthetic LIGH DATA bytes `[... 36 74 66 00 ...]` asserts `color ≈ (0.4, 0.455, 0.212)` (R, G, B = 0x66/0xFF, 0x74/0xFF, 0x36/0xFF).

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 3 C2 (also surfaced by Dim 6 #5).
