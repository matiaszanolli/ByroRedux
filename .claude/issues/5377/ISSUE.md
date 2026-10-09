# #5377: PAR-D2-2026-10-08-01: menuxml `decode_uncompressed` underflows a `u32` on any zero or narrow channel mask, which panics in dev/test builds; release decodes L8 as red

**Labels**: high,import-pipeline,ui,safety,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5377

**Source**: `docs/audits/AUDIT_PARSERS_2026-10-08.md` — `PAR-D2-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: HIGH
- **Dimension**: Error Semantics
- **Location**:
  - `crates/menuxml/src/tex.rs:300-319`: `shift_of` returns `(0, 1)` for a zero mask, and `scale` computes `(v >> (2 * bits - 8).min(bits))`.
  - `crates/menuxml/src/tex.rs:332-343`: the luminance test `r_mask == b_mask && g_mask == r_mask`.
  - Production callers:
    - `crates/menuxml/src/menu.rs:459` (menu art)
    - `crates/menuxml/src/font.rs:88` (font-atlas DDS fallback)
    - `byroredux/src/hud.rs:574` (compass strip)
    - `byroredux/src/commands/assets.rs:421` (`tex.dump`, which decodes any archive texture)
- **Status**: NEW
- **Trigger Input**: an uncompressed DDS header (`DDPF_FOURCC` clear) with `RGBBitCount` 8, 24 or 32 where any of R, G or B is 0, or any non-zero mask is under 4 bits wide. Examples:
  - The standard `DDSPF_L8` header: flags `0x20000`, masks `0xFF, 0, 0, 0`.
  - `DDSPF_A8`: masks `0, 0, 0, 0xFF`.
  - A2R10G10B10: the 2-bit alpha mask `0xC0000000`.
- **Description**:
  1. `bits` is a `u32`. For `bits < 4`, `2 * bits - 8` underflows. The dev profile keeps overflow checks on (`[profile.dev]` sets `opt-level = 1` only), so this panics with "attempt to subtract with overflow".
  2. `scale` runs unconditionally for R, G and B. A zero colour mask, reported as `bits = 1`, therefore always reaches it. Only alpha is guarded, by `a_mask == 0`.
  3. In release the subtraction wraps and `.min(bits)` hides it. The result is still wrong:
     - The luminance branch requires all three colour masks to be equal and non-zero. The standard L8 header has G = B = 0, so it never matches, and an L8 texel decodes as `(L, 0, 0)`.
     - A 2-bit alpha of 3 expands to 192 instead of 255.
  4. Neither the HUD driver nor the debug-command path runs under `catch_unwind`, so in a dev build (`cargo run`, the documented default) the panic takes down the engine's main thread.
  5. The strict real-data lane runs `--release`, so overflow checks are off there. No unit fixture covers an 8-bit or zero-mask header.
- **Evidence**:
  - **Probe** (`menuprobe --bin dds`, real `Rgba8::decode_dds`):
    ```
    (dev)     L8 grey 0x80: PANIC "attempt to subtract with overflow"
    (dev)     A8 0x80: PANIC
    (dev)     A2R10G10B10 white opaque: PANIC
    (release) L8 grey 0x80: Some([128, 0, 0, 255])      # expected [128,128,128,255]
    (release) A8 0x80: Some([0, 0, 0, 128])
    (release) A2R10G10B10 white opaque: Some([255, 255, 255, 192])   # expected alpha 255
    ```
  - **Vanilla census** (`menuprobe --bin ddscensus`, every uncompressed `.dds` header in the textures and misc BSAs):
    - Oblivion has **470** L8 files (flags `0x20000`, masks `000000ff,0,0,0`), for example `textures\clutter\voidessence_g.dds`.
    - No vanilla menu path in Oblivion, FO3 or FNV carries an 8-bit DDS. The vanilla HUD is therefore unaffected, and the vanilla trigger is `tex.dump` on any of those 470 files.
- **Impact**:
  - **Dev/test builds:** an engine abort from one texture. It is reachable from:
    - `tex.dump` on 470 vanilla Oblivion textures;
    - any mod menu art, font atlas or compass strip authored as L8, A8 or a narrow-mask format.
  - **Release builds:** silently wrong colours for the same files: red-tinted luminance and under-opaque 2-bit alpha.
  - The skill's severity rule applies: a panic in an untrusted-input reader is HIGH.
- **Related**:
  - REN-D5-2026-10-08-02: the renderer's `parse_dds` rejects L8; this decoder accepts and then mis-decodes it.
  - REN-D5-2026-10-08-01: the mask gap is not shared.
  - #1542, PAR-D3-2026-10-08-01.
- **Suggested Fix**:
  - Treat a zero mask as "channel absent" and skip `scale`, the way alpha is already handled.
  - Compute the bit replication in signed or saturating arithmetic. Bits replicate as `v8 = v << (8 - b)`, then OR in `v8 >> b` repeatedly until 8 bits are filled.
  - Key luminance off `DDPF_LUMINANCE` (`0x20000`), or off `g_mask == 0 && b_mask == 0 && r_mask != 0` when `bit_count == 8`, and replicate L into R, G and B.
  - Check the payload length against `width * height * bpp` before `Rgba8::new`. Today a 128-byte header claiming 8192² reserves 256 MiB before the first bounds check.
  - Add fixtures for L8, A8 and A2R10G10B10 that run in the debug unit lane.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other DDS decoders (`parse_dds` in the renderer, REN-D5-2026-10-08-01/02))
- [ ] **TESTS**: A regression test pins this specific fix
