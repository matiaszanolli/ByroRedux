# REN-D11-2026-09-21-01: AgX clamps linear input to [0,1] before `log2` — highlights ≥ ~1.0 flatten to 0.59 display-linear, white is unreachable, black takes a `log2(0)` → NaN path

**Labels**: medium, renderer, shaders, test-gap, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: MEDIUM (opt-in display transform visibly wrong; the default ACES path is unaffected) · **Dimension**: FSR/Presentation
**Location**: `crates/renderer/shaders/presentation.frag` `agx()` (~:99-112); Rust mirror `crates/renderer/src/tonemap.rs` `agx()` (~:113) and its tests `saturated_channels_are_monotonic` / `outputs_are_bounded_and_black_maps_to_black`
**Status**: NEW (introduced by `c5663fe39`)
**Verified against**: HEAD `f97775ca8`

## Description

`presentation.frag` `agx()` runs:

```glsl
val = AGX_MAT * val;
val = clamp(val, 0.0, 1.0);   // linear clamp
val = log2(val);
val = (val - min_ev) / (max_ev - min_ev);
```

Minimal AgX (Wrensch 2023), the implementation the file cites, clamps in log space instead: `val = clamp(log2(val), min_ev, max_ev)`. three.js's port also clamps after the log encode. Because the linear value is clamped at 1.0, `log2` never exceeds 0. The `max_ev = 4.026069` headroom (about 4 stops above 1.0) is never used.

The low end is broken too. An exactly-zero channel (black, or a negative input clamped to 0) gives `log2(0) = -inf`. The contrast polynomial then evaluates inf − inf = NaN, and the outset matrix carries it. The only rescue is `max(val, 0.0)`, which lowers to GLSL.std.450 `FMax`, whose result is undefined for a NaN operand. The image-health counter runs before tone mapping and cannot see this.

## Evidence

- Grey ramp evaluated with the shader's own constants (display-linear output):

  | Input | Repo | Reference (log-space clamp) |
  |---|---|---|
  | 0.5 | 0.425 | 0.425 |
  | 1.0 | 0.590 | 0.590 |
  | 1.5 | 0.590 | 0.683 |
  | 2.0 | 0.590 | 0.743 |
  | 4.0 | 0.590 | 0.861 |
  | 16 | 0.590 | 0.995 |

- The Rust mirror `tonemap.rs` `agx()` copies the linear clamp; its doc says "The input clamp to `[0, 1]` matches the GLSL". The comment on `saturated_channels_are_monotonic` calls the clip "reference behaviour".
- Rust's `f32::max(NaN, 0.0)` returns 0.0, so `outputs_are_bounded_and_black_maps_to_black` passes on the Rust side and cannot see the GLSL NaN path.
- `shaders_pin_the_mirror_constants` pins the constants, not the order of operations. The tests are circular: they encode the deviation, not the reference.

## Impact

With `--tonemap agx` or the `tonemap agx` console command, every surface, light and sky texel at or above ~1.0 linear clips to a flat ~0.59 display-linear (~79 % sRGB grey). That defeats the highlight roll-off Stage 1 chose AgX for. Black-pixel output depends on the driver (NaN through `FMax`/`FClamp`). The default ACES path is unaffected.

## Related

- SAFE-D7-2026-09-21-01 (`docs/audits/AUDIT_SAFETY_2026-09-21.md`): the exposure meter's averaging bug. It is a separate defect with its own issue.
- REN-D3-2026-09-21-01 (#4584): the operator id is a hand-written `1u` in the same `tonemap()` dispatch.
- Cited, not re-reported, by `docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`.

## Suggested Fix

- Use the reference log-space clamp, with a small floor before the log: `val = clamp(log2(max(val, 1e-10)), min_ev, max_ev)`.
- Mirror it in `tonemap.rs`.
- Replace the circular tests with two new ones: the grey ramp keeps rising past 1.0 and reaches ≥ 0.99 at 16; an exact-zero input yields a finite value, checked before any `max` so the Rust side cannot mask a NaN.

Do this before any Stage-1 AgX oracle is minted against the current curve.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D11-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: `tonemap.rs` `agx()` mirrors the shader's new order of operations (log-space clamp), not just its constants
- [ ] **SIBLING**: `presentation.frag.spv` recompiled with glslang 11:16.2.0; `scripts/check-shader-artifacts.sh` green
- [ ] **TESTS**: grey ramp monotonic past 1.0 reaching ≥ 0.99 at 16, plus a finite-output assertion for exact-zero input
