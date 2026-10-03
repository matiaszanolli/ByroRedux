# #5219 — REN-D11-2026-10-03-03: presentation and exposure docs went stale after #5154 and #5158 (the "16x clamp" text and `tonemap(graded * exposure)`)

**Labels**: low,renderer,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**:
  - The #5154 comment in `presentation.frag` `main`: "halving saturation at the 16x clamp".
  - The `adaptation_chroma_compress` doc in `crates/renderer/src/tonemap.rs`: "halving saturation at the meter's 16x clamp".
  - The `ADAPTATION_SAT_FALLOFF` comment in `crates/renderer/src/shader_constants_data.rs`.
  - `docs/engine/shader-pipeline.md` (the `presentation.frag` table row and pass 17b/presentation prose) and `docs/engine/renderer.md` (steps 21 and 24, plus the file-tree note). Both still describe presentation as `tonemap(graded * exposureTex)`.
  - The #4591 comment in `record_exposure_meter_pass` (`post_passes.rs`): "Fixed mode (the default)".
- **Status**: NEW (doc rot).
- **Description**:
  - Since 7d99ba7f0, `MAX_AUTO_EXPOSURE` is 2.0, not 16. The maximum auto-mode lift is now about 0.74 stops, which gives chroma of about 0.88, not "halved". The updated test comment in `tonemap.rs` already says this; the three prose sites were not updated.
  - Since 4bf2ec3a4, presentation's tonemapper input is `compressed * exposure`, the luma-preserving chroma compress. The engine docs omit that stage.
  - Auto exposure has been the boot default since a070baaad and 546e7fbc7, so "Fixed mode (the default)" in the meter gate comment is wrong.
- **Impact**: Readers are misled about how strong the #5154 compress is under the envelope, and about the presentation stage list. There is no runtime effect.
- **Suggested Fix**:
  - Restate the three comments as "about 0.88 chroma at the 2× envelope cap; `ev` compensation can lift further".
  - Add the chroma-compress step to the two engine docs.
  - Change "Fixed mode (the default)" to "fixed mode".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)
