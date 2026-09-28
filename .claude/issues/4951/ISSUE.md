# #4951: REN-D2-2026-09-27-03: `interleavedGradientNoise` loses its spatial and temporal resolution over a session (float product `frameCount * 5.588238`)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4951
- **Labels**: low,renderer,shaders,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D2-2026-09-27-03**._

- **Severity**: LOW
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/include/math_common.glsl` (fn `interleavedGradientNoise`); live caller in the default build: `triangle.frag` glass roughness-gated refraction scatter (`rn1`/`rn2`, seeded `cameraPos.w + 37.0` / `+ 53.0`). Compiled-out callers are in the `ENABLE_LEGACY_WRS` arm (candidate `u`, shadow disk `noise1`/`noise2`).
- **Status**: NEW. #1161 (closed) fixed only the u32→f32 cast (`frame_counter & 0xFFFFFF`), not the arithmetic. The 2026-09-23 report flagged this "same float-product shape" as out of its scope.
- **Description**: `fract(dot(fragCoord + frameCount * vec2(5.588238), magic.xy))` is evaluated in f32 with `frameCount` up to 2^24. f32 emulation over a 256×256 pixel block, at 60 fps:

  | Frames (60 fps) | Distinct outputs | Vertically adjacent pixels equal | Distinct over 64 consecutive frames (one pixel) | Mean |
  |---|---|---|---|---|
  | 100 k (~0.5 h) | 256 | — | — | — |
  | 1 M (~4.6 h) | 32 | 81 % | 32 | — |
  | 3 M (~14 h) | 8 | 95 % (52 % horizontal) | 8 | 0.43 (biased) |

  Higher frame rates reach each row proportionally sooner.
- **Evidence**: A numpy float32 mirror of the exact expression is in the audit notes (no repo files written).
- **Impact**: In long sessions, rough-glass refraction scatter turns into structured, banded and slightly biased noise that TAA/FSR cannot average away. The default ReSTIR, shadow jitter and GI paths are unaffected, since they already use the exact integer `hash2_pixel_frame`.
- **Related**: #4776 (same class, fixed for the froxel jitter), #1161.
- **Suggested Fix**: Seed the glass scatter from `hash2_pixel_frame(uvec2(gl_FragCoord.xy), uint(cameraPos.w) …)` like the other stochastic paths, or wrap the IGN frame term to `mod(frameCount, 64.0)` before the multiply.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
