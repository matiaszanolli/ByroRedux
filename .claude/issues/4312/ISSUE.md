# #4312: REN-2026-09-14-D16-02: `62a09fd9` added the bright-pass directly below `BLOOM_INTENSITY`'s doc block but left it asserting there is no bright-pass, and three bloom comments still name `composite.frag` as the bloom consumer

- **Labels**: low,renderer,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4312
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Bloom
- **Location**:
  - `crates/renderer/src/shader_constants_data.rs` (`BLOOM_INTENSITY` doc)
  - `crates/renderer/shaders/bloom_upsample.comp` (header)
  - `crates/renderer/shaders/composite.frag` (the "VOLUME_FAR and BLOOM_INTENSITY now come from the `#include`d header" comment)
- **Status**: NEW
- **Description**:
  1. `BLOOM_INTENSITY`'s doc still reads: "`bloom_downsample.comp`'s `DownsampleParams` carries no bright-pass threshold or Karis average, so this is a broadband lift on the local average, not a highlight-only glow."
     - Since `62a09fd9`, `DownsampleParams::bright_pass` is exactly such a threshold, and the next constant (`BLOOM_THRESHOLD`) documents it.
     - The same doc's "effective contribution … = 0.75× the local blurred average" is the pre-threshold figure; the commit itself measures ~1.01× on the sky after the knee.
  2. That doc says `BLOOM_INTENSITY` is "Consumed by `composite.frag` via the `#include`d `#define`". The only shader reference is `bloom_apply.comp` (`scene.rgb + bloom * BLOOM_INTENSITY`); `composite.frag` mentions it only in a comment.
  3. `bloom_upsample.comp` still says "Final mip 0 is what composite samples" and quotes the 5× × 0.15 "effective contribution to composite.frag's `combined`".
  4. The two adjacent docs now contradict each other on emissives:
     - `BLOOM_INTENSITY` says "See `feedback_color_space.md` for why we don't HDR-boost emissives globally".
     - `BLOOM_THRESHOLD` says "the real fix is the global HDR emissive boost that `feedback_color_space.md` and `BLOOM_INTENSITY`'s note above both already name".
- **Evidence**:
  - `grep -n "BLOOM_INTENSITY" crates/renderer/shaders/*.comp crates/renderer/shaders/*.frag` → `bloom_apply.comp` (code) and `composite.frag` (comment only).
  - The `BLOOM_INTENSITY` vs `BLOOM_THRESHOLD` doc blocks in `shader_constants_data.rs`.
- **Impact**: Documentation only, on the constant a tuner reaches for first. It tells them bloom is an un-thresholded broadband lift (the exact behaviour `62a09fd9` removed, 1.71× sky gain), names the wrong consumer shader, and gives two opposite readings of the emissive-boost policy.
- **Related**: #2805 (closed, contradictory `BLOOM_INTENSITY` derivations), #3608 (closed, `renderer.md` bloom attribution), REN-2026-09-14-D8-02 (`composite.frag` bloom binding comment).
- **Suggested Fix**:
  - Rewrite the `BLOOM_INTENSITY` doc: intensity is applied to the soft-knee-thresholded pyramid, consumed by `bloom_apply.comp`, with the 0.75× figure marked as the un-thresholded DC bound.
  - Update `bloom_upsample.comp`'s "composite samples" wording.
  - Settle the emissive-boost sentence in one place.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
