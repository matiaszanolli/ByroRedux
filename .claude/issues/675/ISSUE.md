# Issue #675: DEN-5: SVGF early-out paths write histAge=1.0 to moments — should be 0 to distinguish "never accumulate" from "first frame"

**File**: `crates/renderer/shaders/svgf_temporal.comp:64-68, 148-151`
**Dimension**: Denoiser & Composite

Both early-out paths (`currID == 0u || (currID & 0x8000u) != 0u` and the no-history fallback at lines 148-151) write `histAge = 1.0` to the moments alpha channel.

When Phase 4's spatial filter lands and reads `moments.b` to scale variance estimation kernels, sky / alpha-blend pixels will be treated as having 1 frame of history when they actually have 0 (no temporal accumulation possible — there's nothing to accumulate). Spatial filter would over-trust them.

**Fix**: Distinguish "early-out, never accumulate" (write `histAge = 0`) from "first frame, will accumulate next frame" (write `histAge = 1`). Both reset to current; only the latter participates in temporal logic.

Phase 4 spec was deferred but `moments_history`'s `.b` channel is reserved for this exact purpose (see svgf.rs:14-15).

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
