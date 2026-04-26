# Issue #674: DEN-4: SVGF temporal α hardcoded to 0.2 — no host-side knob for cell-load / weather-flip discontinuities

**File**: `crates/renderer/src/vulkan/svgf.rs:641-649`
**Dimension**: Denoiser & Composite

The temporal blend α is fixed at 0.2 for both color and moments (`params: [0.2, 0.2, first_frame, 0.0]`). Schied 2017 §4 recommends 0.2 as the canonical floor but expects per-pixel α modulation by history age (the histAge weighted average from #422 already does this in-shader: `alpha' = max(0.2, 1/(age+1))`).

On scene transitions (cell load, weather flip, fast camera turn) the host could pass a higher α — say 0.5 — until variance settles. Today the only "fast" path is `first_frame` (which fully resets) or the implicit per-pixel age recovery.

**Fix**: Wire α into a per-frame parameter on `VulkanContext` so cell-loader / weather change can bump it for ~5 frames after a discontinuity. Stub: pass `alpha_color`, `alpha_moments` from caller.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
