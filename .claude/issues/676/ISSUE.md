# Issue #676: DEN-6: TAA writes 1.0 to alpha — destroys alpha-blend marker bit the main pass writes to HDR.a

**File**: `crates/renderer/shaders/taa.comp:85, 114, 157`, `crates/renderer/shaders/composite.frag:208-209, 279`
**Dimension**: Denoiser & Composite

Composite samples `hdrTex` (= TAA output when TAA is enabled) and uses `direct4.a` as the output color's alpha. TAA writes `vec4(currRgb, 1.0)` and `vec4(outRgb, 1.0)` regardless of input alpha. So whatever the main render pass put in HDR.a (the alpha-blend marker, used as a hint by some downstream paths) is overwritten by TAA.

Today this is harmless because composite only forwards `.a` to the swapchain, which is ignored. But if any future composite logic gates on alpha (glass tint, decal markers) the bit will be silently zeroed.

**Fix**: TAA should preserve alpha:
```glsl
imageStore(uOutput, pix, vec4(outRgb, alpha_passthrough));
```
where `alpha_passthrough = texelFetch(uCurrHdr, pix, 0).a`. Cheap (single extra .a read on the same fetch).

Same logic applies to SVGF — `outIndirect` is RGB-only (B10G11R11_UFLOAT_PACK32, no alpha) so the indirect channel doesn't carry this risk.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
