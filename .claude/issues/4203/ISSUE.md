# PERF-D5-2026-09-11-02: The two FSR mask attachments are unconditional render-pass attachments, so `--upscaler taa` clears/writes 2 B/px/frame nothing samples

Labels: low,performance,renderer,bug

**Description**: The FSR3 reactive/transparency masks (attachments 6/7) are built unconditionally into the fixed 8-attachment main render pass; their sole reader is `record_upscale_pass`, which under `UpscalerMode::Taa` never dispatches FSR. `triangle.frag` declares both outputs with no upscaler-mode specialization. The TAA path is reachable in production (not just via an explicit flag) — it's also the FSR-construction-failure fallback (#2480).

**Evidence**:
`crates/renderer/src/vulkan/context/helpers.rs:178-198,276-277,389-390`, `triangle.frag:69-70`, `gbuffer.rs:88`.

**Impact**: ~7.4 MB/frame ROP traffic + ~14 MB VRAM at 2560x1440 on the non-default path; does not affect the shipped default (`Fsr3(Quality)`).

**Related**: #2480 (the FSR-construction-failure fallback that reaches this path).

**Suggested Fix**: Document only (recommended, zero risk) — do not restructure the render pass speculatively; a real fix (a second render-pass variant) touches every pipeline/framebuffer/shader-output-location in lockstep and must not land without RenderDoc + `BYRO_VALIDATION=1` evidence on both upscaler modes.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
