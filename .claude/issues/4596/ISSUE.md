# SAFE-D5-2026-09-21-01: The CI `vulkan-validation` (lavapipe) gate has never reached Vulkan since at least 2026-08-31, and cannot by design since 2026-09-17

**Labels**: medium, safety, tech-debt, vulkan, bug

Filed via /audit-publish from docs/audits/AUDIT_SAFETY_2026-09-21.md.

**Severity**: MEDIUM (a defence-in-depth gap for the HIGH-floor Vulkan-spec class) · **Dimension**: 5 — Vulkan spec compliance
**Location**:
- `.github/workflows/ci.yml`: job `vulkan-validation` (~:280-359)
- `crates/renderer/src/vulkan/device.rs`: `is_hardware_render_device` (~:311-313) and its use in `pick_physical_device` (~:351-361)

**Status**: NEW
**Verified against**: HEAD `f97775ca8`. CI runs were read with `gh run view --log-failed`.

## Description

- `vulkan-validation` is the only CI lane that boots the engine under `VK_LAYER_KHRONOS_validation`. It is also the only lane that runs a live world under `BYRO_LOCK_ORDER_CHECK=1`.
- Every sampled run dies in "Run 5-frame bench under VK_LAYER_KHRONOS_validation" before any `VkInstance` exists: `thread 'main' panicked at …/xkbcommon-dl-0.4.2/src/x11.rs:59:28: Library libxkbcommon-x11.so could not be loaded.` The apt step installs `mesa-vulkan-drivers`, `vulkan-validationlayers`, `libvulkan-dev`, `libasound2-dev` and `xvfb`, but not `libxkbcommon-x11-0`.
  - The report sampled runs from 2026-08-31 (33451855702), 09-11 (34546452899), 09-21 (35622136101) and HEAD (35658431384).
  - Publish-time re-check: 33451855702 (`06c0ccc1c`, `bench exit status: 101`) and 35658431384 show the same panic.
- So the job is red for a non-Vulkan reason on every run, and a real `[Vulkan]` error would look the same.
- Independently, `33a99ed94` (2026-09-17) made `pick_physical_device` skip `VK_PHYSICAL_DEVICE_TYPE_CPU` devices (`is_hardware_render_device`, which logs "Rejecting Vulkan CPU device …"). Lavapipe is a CPU device. Installing the library would only turn the panic into the "No suitable GPU found" bail that the job tolerates. The next step, `scripts/check-bench-determinism.sh`, would then fail on the non-zero exit.
- No other workflow enables validation:
  - The self-hosted GPU workflows (`rt-correctness.yml`, `playable-smoke.yml`) are `workflow_dispatch`-only and enable no validation layer.
  - The nightly `real-data-gates.yml` enables none either, and has never run (NIF-D3-2026-09-21-01).

## Evidence

```rust
// crates/renderer/src/vulkan/device.rs
fn is_hardware_render_device(device_type: vk::PhysicalDeviceType) -> bool {
    device_type != vk::PhysicalDeviceType::CPU
}
```

- The CI log lines are quoted above.
- `grep -n 'BYRO_VALIDATION\|VK_INSTANCE_LAYERS\|VK_LAYER_KHRONOS' .github/workflows/*.yml` matches only the `vulkan-validation` job in `ci.yml`.

## Impact

- No CI lane can see a Vulkan validation error. That includes SAFE-D1-2026-09-21-01 (#4592) and SAFE-D2-2026-09-21-01 (#4593), and #4510's VUID pair, which only a manual run found on 2026-09-20.
- The lock-order detector's only live-world lane is inert in the same way. That half belongs to the concurrency audit (CONC-D3-2026-09-21-01 covers the test-suite lane).

## Related

- #2138 (closed): the job used to swallow exit codes.
- SAFE-D4-2026-09-21-01 (#4595): the clippy gate is inert for a related reason.
- CONC-D3-2026-09-21-01 (`docs/audits/AUDIT_CONCURRENCY_2026-09-21.md`) and NIF-D3-2026-09-21-01 (`docs/audits/AUDIT_NIF_2026-09-21.md`): other inert CI lanes found today.

## Suggested Fix

- Add `libxkbcommon-x11-0` to the apt step.
- Then do one of two things:
  - add a CI-only switch that admits CPU devices for the 5-frame validation boot, on a Mesa new enough to expose ray query;
  - or move the gate to the self-hosted RT runner with `BYRO_VALIDATION=1`.
- Until one of those lands, the `/audit-safety` Dim-5 first step should say the gate is inert.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D5-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: the `Assert renderer-static scene-state determinism` step re-checked against the chosen device policy
- [ ] **TESTS**: a main-branch run shows the bench reaching Vulkan device selection under the validation layer (a clean `[Vulkan]` log, or a validation error that fails the job)
