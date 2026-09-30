# #4862: REN-D11-2026-09-24-02: #4578's fix has no GLSL-side regression guard, and the mirror docs still state the premise the fix removed

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D11-2026-09-24-02**._

- **Severity**: LOW (test gap plus doc rot on correct code).
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/tonemap.rs` — `shaders_pin_the_mirror_constants`, the `agx` doc comment, `saturated_channels_are_monotonic` doc; `crates/renderer/shaders/presentation.frag` header ("Behavioural mirror + licence pin").
- **Status**: NEW (follow-up to closed #4578)
- **Description / Evidence**: The fix is correct in the shipped artifact (`spirv-dis` shows `FMax(val, 1.0e-10) → Log2 → FClamp(_, -12.4739, 4.02607)`; an independent evaluation gives 0.425 / 0.590 / 0.683 / 0.743 / 0.861 / 0.995 at input 0.5 / 1 / 1.5 / 2 / 4 / 16 and exactly 0 at black). But the only GLSL-side pin is substring presence of constants: re-introducing `val = clamp(val, 0.0, 1.0); val = log2(val);` keeps every constant present, so every test stays green. The Rust doc still says "inset → clamp → log2" and "The input clamp to `[0, 1]` matches the GLSL" (the false premise #4578 deleted), and the shader comment mentions a "licence pin" no test provides.
- **Suggested Fix**: Add a source pin for the log-space clamp order (runtime-composed needle), correct the two doc comments, and drop or implement the "licence pin" wording.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

