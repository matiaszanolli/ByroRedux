# Issue #4987

**Title:** CONC-D3-2026-09-28-03 (regression of #4596): The `vulkan-validation` CI lane has never reached a Vulkan device since #4596 closed, so the only live-world run of the lock-order detector (and of the validation layer) is inert
**State:** OPEN
**Labels:** bug, medium, concurrency, test-gap

- **Severity**: MEDIUM. This is the same defence-in-depth rating #4596 carried. It covers the HIGH-floor classes "ECS deadlock" and "Vulkan spec violation".
- **Dimension**: ECS Lock Ordering. The lane is the dynamic supplement to the static scheduler proof.
- **Location**:
  - `.github/workflows/ci.yml:312-360`, the job `vulkan-validation`.
  - `crates/renderer/src/vulkan/instance.rs:48`, `api_version(API_VERSION_1_3)`.
- **Status**: Regression of #4596 (closed 2026-09-22 by `dc44de735`). Its completeness check "a main-branch run shows the bench reaching Vulkan device selection under the validation layer" is unmet.
- **Description**:
  - `dc44de735` added `libxkbcommon-x11-0` and `BYRO_ALLOW_CPU_VULKAN_DEVICE=1`. That fixed the old winit panic, and the lane now fails one step later.
  - Every sampled `main` run dies at `vkCreateInstance` with `ERROR_INCOMPATIBLE_DRIVER`. ash renders that as "Unable to find a Vulkan driver". It is the loader's result when no usable ICD is found.
  - The job then fails on the engine panic described in CONC-D3-2026-09-28-04.
  - What never runs as a result:
    - No `VkPhysicalDevice` is ever enumerated.
    - No frame is rendered under `VK_LAYER_KHRONOS_validation`.
    - The rayon parallel batch never runs against a real loaded world under `BYRO_LOCK_ORDER_CHECK=1`. `ci.yml:314-321` calls that "exactly the workload the cross-thread lock-order graph was built for".
  - Both pin tests stay green because they check only the YAML text: `vulkan_validation_job_enables_the_lock_order_detector` (`byroredux/src/scheduler_access_tests.rs:370`) and `vulkan_validation_job_fails_on_a_panic` (`:390`).
- **Evidence**: `gh run view <id> --log` for the Vulkan-validation job. Eight of eight sampled runs show the identical signature: 122121a27 (09-24), 5c82276df, 9e6f8a978, 078f650ec, e26441c34, bad6ef2e2, 7e9da5dcc, efc059f3a, and a070baaad (run 36499415560, 09-28). The log reads:
  ```
  ERROR byroredux::app_events] Vulkan init failed: Failed to create Vulkan instance: Unable to find a Vulkan driver
  thread 'main' panicked at crates/core/src/ecs/world.rs:713:13:
  Resource `byroredux_scripting::papyrus_demo::PapyrusPlayerEntity` not found
  bench exit status: 101
  ```
- **Trigger Conditions**: Every CI run on `main`.
- **Impact**:
  - Every cross-thread lock-order claim for the real parallel batch rests only on the static access proof (Dim 4) and on reading the code (Dims 3 and 5). This audit's Dims 3–5 found no live hole, but no dynamic evidence backs that.
  - No CI lane can see a `VUID-*` or sync-validation hazard. That includes the HYPOTHESIS rows of this report: CONC-D1-2026-09-28-01 and CONC-D2-2026-09-28-02.
- **Verification Path**: CI. A green lane run must show a `Selected physical device` log line and 5 rendered frames.
- **Related**: #4596, #2138, #1429; CONC-D3-2026-09-28-04 and -05.
- **Suggested Fix**:
  - Add `VK_LOADER_DEBUG=error,warn,driver` and `ls /usr/share/vulkan/icd.d/` to the step, so the missing or incompatible ICD is named. The `lvp_icd.x86_64.json` path on ubuntu-24.04 is unverified, and newer Mesa may ship `lvp_icd.json`. Consider `VK_DRIVER_FILES`.
  - Add a lane assertion that grep-fails when no device-selection line appears, so a lane that cannot boot turns red for the right reason.

> Filed as a **regression of #4596** (closed 2026-09-22 by `dc44de735`): that issue's own `TESTS` completeness check — a main-branch run reaching Vulkan device selection — has never been met, so the closed issue does not track the current failure.


Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D3-2026-09-28-03) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems / CI steps
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition and the `docs/engine/ecs.md` canonical order are preserved
- [ ] **TESTS**: A regression test pins this specific fix

