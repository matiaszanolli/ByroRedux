# #5254: REN-D11-2026-10-05-01: `exposure_meter.comp` still hand-types the S/K = 8.0 meter calibration that Rust derives from `SENSOR_SENSITIVITY_S` / `LIGHT_METER_CALIBRATION_K`

**Labels**: low,renderer,shaders,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5254

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-05.md` — `REN-D11-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/shaders/exposure_meter.comp` (`float ev100 = log2(max(avg_luminance, 1.0e-6) * 8.0);`); `crates/renderer/src/vulkan/exposure.rs` (`ev100_from_average_luminance`, `EXPOSURE_CONSTANT` doc).
- **Status**: NEW. This is an incomplete fix of #5218, which is closed.
- **Description**: `fc73e0a66`'s message names three hand-typed meter values: the 1.2 neutral, "the 8.0 S/K constant" and the Rec.709 weights. It single-sources only the neutral and the weights. S and K live in `exposure.rs`, not in `shader_constants_data.rs`, so the generated header cannot carry them, and no test ties the shader's literal to their ratio.

  The `EXPOSURE_CONSTANT` doc still claims the meter, the chroma compress and the host "cannot disagree". Retuning K (ISO 2720 allows 12.5 to 14) would move `auto_exposure` and the #5158 envelope tests but not the GPU meter.
- **Suggested Fix**: Move S and K, or their ratio, into `shader_constants_data.rs` and read it in the shader. Alternatively, add a source pin in `exposure_meter.rs` that the shader's factor equals `SENSOR_SENSITIVITY_S / LIGHT_METER_CALIBRATION_K`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
