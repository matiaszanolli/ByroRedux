# Issue #667: SH-12: composite caustic fixed-scale magic number (65536.0) duplicated across 3 sites with no shared constant

**File**: `crates/renderer/shaders/composite.frag:47`, paired with `crates/renderer/src/vulkan/caustic.rs` and caustic_splat.comp
**Dimension**: Shader Correctness

`const float CAUSTIC_FIXED_SCALE = 65536.0;` is hardcoded in composite.frag. Comment at line 46 explicitly notes "Kept in sync manually; if it changes in caustic.rs, the layout test there will not fail (it's Rust-only), so update this constant." Same value lives in `caustic_splat.comp` as `causticTune.x` (uniform), but composite.frag uses the GLSL const instead of reading it from a UBO.

Drift risk: someone tunes Rust-side, leaves shader unchanged, caustic luminance is off by a factor.

**Fix**: Either:
- Route the constant via the existing `CompositeParams` UBO (one extra float in `cloud_params_3.w`-equivalent slot), OR
- Add a Rust-level `const_assert!(CAUSTIC_FIXED_SCALE == 65536.0)` in caustic.rs and a build-time test that greps composite.frag for the literal.

Comment-only fix is inadequate — three sites of the same magic number without a single source of truth.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
