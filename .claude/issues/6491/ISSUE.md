# REG-2026-10-27-01: Regression of #5096: `render_one_frame` outgrew its size pin, and the `cargo test` step has been red since 2026-10-27

Labels: medium, tech-debt, bug
Source: docs/audits/AUDIT_REGRESSION_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_REGRESSION_2026-10-27.md` -- finding `REG-2026-10-27-01` (MEDIUM)
**Location (re-mapped to current code)**: `byroredux/src/app_frame.rs` (`fn render_one_frame`, 728 lines at HEAD, re-measured); pin in `byroredux/src/scheduler_access_tests.rs` (`per_frame_driver_functions_stay_within_their_size_pins`, entry `(APP_FRAME_RS, "fn render_one_frame(", 716)`).
**Status at filing**: Regression of #5096 (CLOSED; no open issue tracks this -- #6143 is open but its remaining scope is the skinned-BLAS refit and `build_blas_batched`, not this pin)

## Description
The #5096 fix (`a4ef215d4`) pinned the per-frame drivers' sizes. The pin held at 712 lines through `296d21912`. `e71451d08` (10-26, terrain distance softening) took `render_one_frame` to 715; `22d6e1a80` (10-27, "implement geometry admission tiers and pool reserve management", `app_frame.rs` +14/-1) took it to 728. The test fails with: `` `fn render_one_frame(` carries 728 lines (pin 716) -- move the new work into a helper or module and record the decision, rather than raising the pin (#5096 / #6143) ``.

## Evidence
Fails locally in the full workspace run and in the CI `cargo test` step of every run from `22d6e1a80` to `eae8fd740`. Brace-count measurement: 79c8322f4 = 712, 296d21912 = 712, e71451d08 = 715, 22d6e1a80 = 728, HEAD = 728.

## Impact
`cargo test` on `main` is red, which masks any other test regression and keeps the release gate (#6331) shut (tag v0.104 was refused). The growth itself is what the pin exists to stop: +16 lines in two days.

## Related
#5096, #4342, #6143 (open), #6359 (open; same class, different function).

## Suggested Fix
Move the geometry-admission tier wiring from `22d6e1a80` into a `streaming_helpers` (or `App`) method that `render_one_frame` calls. Do not raise the pin.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

