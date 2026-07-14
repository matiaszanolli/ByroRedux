**Severity**: low
**Dimension**: Bloom (renderer audit 2026-07-14, DIM16)
**Location**: `crates/renderer/shaders/bloom_upsample.comp`, DC-gain note (#1275), line ~22
**Status**: NEW (CONFIRMED against HEAD)

## Description
The DC-gain note (introduced by `d11704da`) states *"a DC-constant scene accumulates up to ~8x peak at up[0]"*. The **mechanism** it describes matches the code exactly (`upsampled` is unit-gain — 4 taps × 0.25 — and `same` is unit-gain, summed with no renormalisation), but the aggregate figure is wrong for the shipped `BLOOM_MIP_COUNT = 5` pyramid. Because each up-step ADDS one unit-gain same-res down mip to a unit-gain upsample of the accumulator, the DC ceiling is **linear, not geometric**: seed `down[4]=V`; `up[3]=2V`, `up[2]=3V`, `up[1]=4V`, `up[0]=5V`. True ceiling ≈ 5×, not ~8×.

## Evidence
- Down mips are independent box-downsamples of the original, so each `down[i]=V` for a DC input.
- `bloom.rs::BloomFrame::new` seeds `up[3]` from `down[4]` and each `up[i]` from `up[i+1]` + `down[i]`.
- Upsample of a constant field = `4 taps × 0.25 = V` (unit gain), added to `same` (`down[i]`, unit gain), no renormalisation (`upsampled + same`).
- `bloom_upsample.comp:22` — the "~8x peak at up[0]" note.

## Impact
None on rendering — `BLOOM_INTENSITY = 0.15` is empirically tuned on real content, not derived from the stated multiplier. Concern is only that a future reader re-deriving from "~8x" gets a wrong number.

## Suggested Fix
Reword to *"accumulates linearly to ~5× peak at up[0] for the 5-mip pyramid (seed + 4 unit-gain additions)"*.

## Completeness Checks
- [ ] **SIBLING**: If `BLOOM_MIP_COUNT` ever changes, restate the ceiling as `(MIP_COUNT)×` rather than a hardcoded figure.
- [ ] **TESTS**: N/A (comment-only).
