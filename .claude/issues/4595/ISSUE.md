# SAFE-D4-2026-09-21-01: The CI gate for `#![deny(clippy::undocumented_unsafe_blocks)]` has not run since at least 2026-09-15

**Labels**: medium, safety, tech-debt, ui, bug

Filed via /audit-publish from docs/audits/AUDIT_SAFETY_2026-09-21.md.

**Severity**: MEDIUM (a defence-in-depth gap around the MEDIUM-floor "unsafe without a SAFETY comment" rule) · **Dimension**: 4 — Unsafe-block discipline
**Location**:
- `.github/workflows/ci.yml`: job `cargo-test` ("Test + Check + Clippy"), step order `cargo test` → `cargo clippy` (~:169-172)
- `crates/renderer/src/lib.rs`: `#![deny(clippy::undocumented_unsafe_blocks)]` (~:21)

**Status**: NEW
**Verified against**: HEAD `f97775ca8`. CI runs were read with `gh api` and `gh run view --log-failed`.

## Description

- The renderer's `#![deny(clippy::undocumented_unsafe_blocks)]` is present, with no `allow` escape. It is a Clippy tool-lint, inert under `cargo build` and `cargo test`. CI enforces it only through `cargo clippy --workspace -- -D warnings`.
- That step runs after `cargo test --workspace` in the same job and has no `if: always()` or `if: success() || failure()`. Any `cargo test` failure skips it.
- `cargo test` has been red on main for weeks, so the clippy step has not run:
  - The report sampled 10 main runs from 2026-09-15 to 2026-09-21; all show `cargo clippy = skipped`. In 9 of them `cargo test` failed. In the tenth (`d54382415`, run 35650571724), `cargo check` failed first, because `exposure_meter.comp.spv` was not yet committed.
  - Publish-time re-check: 45 sampled main runs, from 2026-09-05 through HEAD's run 35658431384, all show `cargo clippy = skipped`.
- At HEAD the first failing test binary is `byroredux_ui`. Eight tests build a real Ruffle player and panic with "Failed to create wgpu device: Ruffle requires hardware acceleration, but no compatible graphics device was found supporting Vulkan":
  - five `navigator::tests::*` tests;
  - two `player::resource_loads_tests::*` tests;
  - `player::render_failure_tests::render_leaves_dirty_set_and_returns_none_on_a_size_mismatch`.
- The same 8 failed on 2026-09-05 (run 33982391005) and 2026-09-15 (run 34967746751).

## Evidence

- HEAD run 35658431384 (`gh api …/actions/runs/35658431384/jobs`): `cargo check = success`, `cargo test = failure`, `cargo clippy = skipped`. The test log shows `test result: FAILED. 74 passed; 8 failed` in `byroredux_ui`.
- The gate is inert in practice, not just in theory. `dc306a6a0` (2026-09-18) landed `with_one_time_commands(device, queue, command_pool, |cmd| unsafe {` in `Texture::overwrite_rgba_pixels` with no SAFETY comment. CI never flagged it; the 2026-09-21 tech-debt audit fixed it inline (#4567).
- `cargo test` runs without `--no-fail-fast`, so it stops at the first failing binary. The UI failures are the first red, not necessarily the only one. On 2026-09-11 (run 34597216311) a different binary failed first: `tools/byro-launcher`'s `engine::tests::supervision::a_crash_carries_its_code_and_the_last_thing_the_engine_said`.

## Impact

- Renderer `unsafe` without a SAFETY comment can merge unseen, and already did once this week.
- Every other `-D warnings` lint in the workspace is unenforced in the same way.
- Nothing is outstanding at HEAD. The audit ran `cargo clippy -p byroredux-renderer --no-deps -- -A clippy::all -D clippy::undocumented_unsafe_blocks` locally, and the renderer is clean.

## Related

- #4567 (closed) treats the gate as "red on the current toolchain" and does not record that CI skips it. #4090 and #4130 (closed) were earlier clippy-red episodes.
- SAFE-D5-2026-09-21-01 (#4596): the `vulkan-validation` lane is inert too.
- CONC-D3-2026-09-21-01 (`docs/audits/AUDIT_CONCURRENCY_2026-09-21.md`): the same red `crates/ui` tests keep the `lock-order-check` lane red. That is a separate consequence, tracked separately.
- NIF-D3-2026-09-21-01 (`docs/audits/AUDIT_NIF_2026-09-21.md`): the nightly real-data lane has never executed, another inert CI gate found today.

## Suggested Fix

- Decouple clippy from the test result: give it its own job, add `if: success() || failure()` to the step, or run it before `cargo test`.
- Make `cargo test` able to go green: gate the adapter-dependent `crates/ui` tests behind a wgpu adapter probe or `#[ignore = "needs a Vulkan adapter"]`. Then run once with `--no-fail-fast` to see whether any later binary is also red.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D4-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every other step sequenced after a fail-fast `cargo test` checked for the same skip-on-failure masking (the `lock-order-check` job is CONC-D3-2026-09-21-01)
- [ ] **TESTS**: a main-branch run after the change shows the `cargo clippy` step executing (success or failure, not `skipped`)
