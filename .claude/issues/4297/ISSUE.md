# #4297: REN-2026-09-14-D23-02: blades write reactive = 1.0 and transparency&composition = 1.0 over every covered pixel, contradicting the FSR integration plan's mask contract (0.9 clamp, opaque geometry masks off, "start material-driven rather than markin…

- **Labels**: medium,renderer,shaders,terrain-exterior,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4297
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: MEDIUM
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/shaders/groundcover_blade.frag` (`main`, `outFsrReactive = 1.0; outFsrTransparency = 1.0;`); `crates/renderer/src/vulkan/groundcover.rs` (blade pipeline `blend_attachments[6]`/`[7]` `color_write_mask(R)`); `docs/engine/fsr3-upscaler-integration-plan.md` §1.4; `docs/engine/fsr3-troubleshooting.md`
- **Status**: NEW
- **Description**:
  - The blade pipeline is opaque and depth-writing, yet it enables R writes on attachments 6/7, and the fragment shader writes both FSR masks at full strength for every blade fragment.
  - The shader header calls this "the documented remedy". The project's authoritative FSR docs say otherwise:
    - The plan's §1.4 table says reactive is written as `min(alpha, 0.9)` by glass, particles, water and alpha-blended decals.
    - The same table says the T&C mask should "Start material-driven rather than marking the entire frame".
    - `fsr3-troubleshooting.md` says "opaque geometry masks its writes off entirely".
  - Neither doc mentions ground cover, and `docs/engine/exal-groundcover.md` has no reactive/transparency/FSR text.
  - An exterior meadow therefore marks most of the lower frame as fully reactive plus T&C. FSR then discards history over that area and reconstructs it from single render-resolution samples, which is the opposite of the §6 goal of letting reconstruction antialias thin blades.
- **Evidence**: The quoted doc lines above; `groundcover_blade.frag` writes `outFsrReactive = 1.0; outFsrTransparency = 1.0;` unconditionally; `groundcover.rs` sets `color_write_mask(vk::ColorComponentFlags::R)` for attachments 6 and 7. The rationale the shader gives (no motion vector for wind-animated geometry) is real, but the plan's own contract answers that case with the 0.9 clamp and material-driven marking, not 1.0 everywhere.
- **Impact**: Visual, and possibly deliberate. The masks are only consumed on the FSR path, which is the engine default. The expected cost is aliasing/shimmer on grass at Quality/Balanced/Performance, compounded by D23-01. It needs an A/B capture: masks at 1.0, at 0.9, and reactive-only. If 1.0 is the measured best choice, the finding reduces to doc drift in both FSR docs.
- **Related**: D23-01; #2749 (triangle.frag early-return mask writes, closed); #4203 (mask attachments unconditional); D11-01 (ground-cover debug pipeline write mask).
- **Suggested Fix**: Decide the policy from a capture: clamp to 0.9 per the plan, and/or drop T&C and keep a reduced reactive value keyed to wind sway. Then record the ground-cover row in `fsr3-upscaler-integration-plan.md` §1.4 and `fsr3-troubleshooting.md`, so the "opaque geometry masks off" statement is no longer contradicted by a live opaque writer.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
