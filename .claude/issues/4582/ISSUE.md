# REN-D2-2026-09-21-01: the RESTIR_LIGHT debug block re-binds `triangle.frag`'s legacy-WRS `#if ENABLE_LEGACY_WRS else` from `if (useRestir)` to `if (viewRestirLight)`

**Labels**: low, renderer, shaders, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (evaluation build only: `ENABLE_LEGACY_WRS == 1`; the default build compiles the `else` out) · **Dimension**: Ray Queries
**Location**: `crates/renderer/shaders/triangle.frag`:
- the ReSTIR finalize `if (useRestir) { … }` (~:3414);
- the RESTIR_LIGHT view block `if (viewRestirLight) { … return; }` (~:3831-3842);
- then `#if ENABLE_LEGACY_WRS` / `else {` legacy pass 2 (~:3843-3982), closed by `} // end legacy WRS pass-2 (else of useRestir)`.

**Status**: NEW (introduced by `f97775ca8`)
**Verified against**: HEAD `f97775ca8`

## Description

`f97775ca8` put the RESTIR_LIGHT debug block after the ReSTIR finalize and before the preprocessor-gated legacy pass 2:

```glsl
        if (useRestir) { /* finalize */ }
        if (viewRestirLight) { …; return; }
#if ENABLE_LEGACY_WRS
        else {
        // ── Pass 2: shadow rays for sampled reservoirs ──
        …
        } // end legacy WRS pass-2 (else of useRestir)
#endif
```

In a build with `ENABLE_LEGACY_WRS == 1` (the documented A/B evaluation build), that `else` now binds to `if (viewRestirLight)`, not `if (useRestir)`. So legacy pass 2 runs whenever the view is off, including on ReSTIR pixels. The closing comment "else of useRestir" is false.

**Publisher's validation note (impact correction).** The audit report says this double-counts direct light. The code does not support that today:
- The legacy reservoir arrays are written only on the legacy streaming branch. `resLight[s] = i` (~:3406) sits inside the `else` of pass 1's `if (useRestir)`.
- On ReSTIR pixels every `resLight[s]` therefore stays `0xFFFFFFFF`. The re-bound pass 2 hits `continue` on all 16 slots and subtracts nothing.

The defect is real but latent:
- 16 wasted loop iterations per ReSTIR pixel in the A/B build, a small skew in the timing that build exists to compare;
- a false structural comment;
- a trap: any future change that fills the legacy reservoirs on ReSTIR pixels would silently double-apply shadowing.

## Evidence

- The source layout above.
- `grep -n "resLight\[" triangle.frag` shows writes only in the init loop (~:3136) and the legacy stream (~:3406), and a read in pass 2 (~:3856).
- `ENABLE_LEGACY_WRS` defaults to 0 (`shader_constants.glsl`), so the shipped SPIR-V is unaffected.

## Impact

There is no visible change in the shipped build. The `ENABLE_LEGACY_WRS == 1` evaluation build has wasted pass-2 iterations on ReSTIR pixels, a false comment, and the latent double-shadowing hazard described in the validation note.

## Related

- REN-D12-2026-09-21-01 (#4577): the RESTIR_LIGHT view itself renders magenta until `composite.frag.spv` is rebuilt.
- #1799 (closed): introduced the `ENABLE_LEGACY_WRS` compile-time gate.

## Suggested Fix

Make the `else` bind to `if (useRestir)` again. Either move the `if (viewRestirLight) { … return; }` block after the legacy pass-2 `#endif` (it reads only `useRestir`/`restirY`, which finalize has already settled), or turn the legacy arm into an explicit `if (!useRestir) { … }` instead of a dangling `else`. Optionally, extend the `ENABLE_LEGACY_WRS` source-shape guard in `crates/renderer/src/shader_constants.rs` to assert that the gated legacy arm is keyed on `useRestir`.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D2-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: the FACING_RATIO block and the other debug early-returns checked for the same `#if … else` re-binding
- [ ] **SIBLING**: `triangle.frag.spv` recompiled (plain `-V`, keeps OpName for the reflection test) if default-build code moves
- [ ] **TESTS**: source-shape pin that the legacy pass-2 arm is keyed on `useRestir`
