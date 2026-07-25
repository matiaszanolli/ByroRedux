## Issue 2130 [OPEN] SCR-D7-NEW3-01: quest_advance_system's one-signal-per-entity assumption is unenforced, true today only by coincidence
labels: bug low info 

**Dimension**: Engine Attach Path & Trigger-Volume Wiring
**Untrusted-Input**: No
**Location**: `crates/scripting/src/papyrus_demo/quest_advance.rs:235-335`
**Status**: NEW (found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md`, Dimension 7 — informational, not reachable today, no fix required now)

## Description

`quest_advance_system` collects `(entity, activator/triggerer)` pairs from both `ActivateEvent` and `OnTriggerEnterEvent`, implicitly assuming a given entity never receives both in the same frame. This holds today only because (1) a `TriggerVolume` is only ever attached to a mesh-less REFR, so the mesh-bearing/mesh-less component sets are disjoint by construction, and (2) `ActivateEvent` has no live automatic emitter yet (only a debug console command) — the real "player activates a REFR" system (`boot.rs`'s "Stage 4") is unbuilt.

The recognizer test `on_activate_wins_over_on_trigger_enter` proves a single script can legitimately define both handlers, so once Stage 4 lands, if it doesn't explicitly exclude `TriggerVolume`-bearing entities from activation eligibility, the disjointness assumption breaks and a single player action could double-fire `QuestStageAdvanced` (idempotent for the stage value, but a genuine double-application risk for a non-idempotent fragment effect like `AddItem`).

## Impact

None today — both preconditions independently hold. Purely forward-looking.

## Suggested Fix

No code change needed now. When Stage 4 (the real "player activates a REFR" system) lands, either exclude `TriggerVolume`-bearing entities from activation eligibility, or add a per-frame per-entity dedup in `quest_advance_system`. A cheap regression test for that future work: insert both event types on the same entity in one frame, assert exactly one `QuestStageAdvanced` marker results.

## Completeness Checks
- [ ] **TESTS**: When Stage 4 lands, add a regression test inserting both `ActivateEvent` and `OnTriggerEnterEvent` on the same entity in one frame, asserting exactly one `QuestStageAdvanced` marker

---
## Issue 2137 [OPEN] CONC-D4-NEW-01: The only CI job that boots the real engine does not enable the ABBA lock-order detector
labels: bug medium sync 

## Severity
MEDIUM

## Dimension
Scheduler Access Declarations — `/audit-concurrency` 2026-07-25

## Location
`.github/workflows/ci.yml:131-173` (`vulkan-validation`) vs `.github/workflows/ci.yml:75-87` (`lock-order-check`)

## Description
`BYRO_LOCK_ORDER_CHECK=1` is set only on the `lock-order-check` job (`cargo test --workspace`, single-threaded hand-built `World`s). The `vulkan-validation` job — the only CI job that boots the actual engine (`cargo run -p byroredux -- --bench-frames 5` under lavapipe, debug profile so `global_order` is compiled in) — does not set the env var, so the detector is compiled in but inert.

Confirmed against current `.github/workflows/ci.yml`: the `vulkan-validation` step exports only `VK_ICD_FILENAMES`, `VK_INSTANCE_LAYERS`, `RUST_LOG` — no `BYRO_LOCK_ORDER_CHECK`.

## Evidence
```yaml
# ci.yml vulkan-validation step
export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json
export VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation
export RUST_LOG=error
```
No `BYRO_LOCK_ORDER_CHECK` anywhere in that job. `ENABLED` is a `LazyLock<AtomicBool>` seeded from `std::env::var_os` at first touch (`lock_tracker.rs:216-217`).

## Impact
The live 5-frame run is the only place in CI where rayon actually dispatches the real parallel batch across worker threads against a real loaded world — precisely the workload the cross-thread graph was built for — and it is the one place the detector is switched off. This is the structural reason CONC-D5-01/-02/-03 (#2134, #2135, and the sibling issue filed above) were never caught: they require a live `PhysicsWorld`/real cell, which only this job provides, and this job runs with the detector disabled.

## Related
#1410 (TS-02, closed — built the detector), same class as CONC-D5-01 (#2134), CONC-D5-02 (#2135), CONC-D5-03, CONC-D4-NEW-03 (filed separately).

## Suggested Fix
Add `BYRO_LOCK_ORDER_CHECK: 1` to the `vulkan-validation` job's `env:` block (or add a second `--bench-frames` invocation with it set). Cost is negligible for a 5-frame run.

## Completeness Checks
- [ ] **TESTS**: Re-run CI to confirm the detector now fires on the current (pre-fix) code and stays quiet once CONC-D5-01/-02/-03 land

---
## Issue 2138 [OPEN] CONC-D4-NEW-02: vulkan-validation CI job swallows the boot-time access-invariant debug_assert failures
labels: bug medium sync 

## Severity
MEDIUM

## Dimension
Scheduler Access Declarations — `/audit-concurrency` 2026-07-25

## Location
`.github/workflows/ci.yml:163-172`; guards at `byroredux/src/boot.rs:1002-1030`

## Description
The three #1394/#1602 guards (`undeclared_parallel_count`/`known_conflict_count`/`unknown_pair_count`, all `debug_assert_eq!(..., 0)`) live in `install_runtime_registries`, called from `App::new` — before the event loop, so they do execute in the `vulkan-validation` job. But the step runs `OUTPUT=$(... cargo run ... 2>&1 || true)` and fails **only** if the output contains the literal substring `[Vulkan]`. A `debug_assert` panic's text contains no such substring, so the job goes green on a tripped guard.

## Evidence
Confirmed against current `.github/workflows/ci.yml`:
```bash
OUTPUT=$(xvfb-run --auto-servernum cargo run -p byroredux -- --bench-frames 5 2>&1 || true)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -qF '[Vulkan]'; then
  exit 1
fi
```
`|| true` swallows any non-zero exit code (including a panic); the sole failure predicate is the `[Vulkan]` substring match. Panic text from `boot.rs:1011/1023/1029` contains no `[Vulkan]` marker.

## Impact
These guards are the primary regression pin for the whole scheduler-access-declaration dimension, and they are currently enforced by nothing in CI: `cargo test` never calls `build_scheduler` (it's `pub(crate)`, sole caller `App::new`), and the one job that does call it discards the exit code. A future `add_to()` or a new conflicting pair (the exact #1601 shape) would reach `main` with a green CI. Today's state is fine (verified statically), so this is a guard-integrity gap, not a live defect.

## Related
#1394 (closed), #1601 (closed), #1602 (closed), `byroredux/src/scheduler_access_tests.rs`, CONC-D4-NEW-01 (same job, adjacent gap, filed separately).

## Suggested Fix
Cheapest — also fail the step on a `panicked at` substring, or capture the real exit code (`set -o pipefail`, keep `|| true` only for the known "no suitable device" bail, matched explicitly). Sturdier — since `scheduler_access_tests.rs` is already compiled into the bin's test binary, add a real `cargo test` asserting the three counts are 0, replacing the `include_str!`-grep proxies.

## Completeness Checks
- [ ] **TESTS**: A regression test / CI change pins this specific fix

---
## Issue 2140 [OPEN] CHAIN-D2-03: FSR dispatch-failure recovery assumes zero partial recording by the SDK before it errors — HYPOTHESIS
labels: bug medium sync 

## Severity
MEDIUM

## Dimension
Compute → AS → Fragment Chains — `/audit-concurrency` 2026-07-25

## Status note
HYPOTHESIS — not reachable by `cargo test`; needs a fault-injection harness before any code change.

## Location
`crates/renderer/src/vulkan/frame_upscaler.rs:441-468`, `:667-698` (`record_fsr_depth_restore`)

## Description
When `context.dispatch` returns `Err`, the recovery path latches `dispatch_failure` and records depth-restore + native-blit barriers whose declared `old_layout` values are correct only if the SDK recorded zero image transitions into `cmd` before failing. `ExecuteGpuJobsVK` in the vendored SDK (`third_party/fidelityfx-sdk-v1.1.4/sdk/src/backends/vk/ffx_vk.cpp:4187-4240`) iterates every queued GPU job and records each into the command buffer, checking `errorCode` only **after** the loop, with the code overwritten each iteration — so a mid-sequence failure can leave partially-recorded transitions while reporting `FFX_OK`, or an error can arrive after real work was already recorded.

## Evidence
`frame_upscaler.rs:453-457` SAFETY comment: "`record_fsr_barriers_before` established the exact layouts these two transition out of" — true only under the zero-partial-recording assumption. `blit_output_src_access` (`:812-818`) encodes the same assumption in code.

## Impact
On a real SDK dispatch rejection, this could produce a device loss or corrupted frame instead of the intended graceful degradation to the native blit — a crash-on-crash in the exact path meant to handle "something already went wrong."

## Trigger Conditions
Any `ffxFsr3UpscalerContextDispatch` failure — SDK OOM, invalid descriptor, device-lost mid-frame. Rare, never exercised on the happy path.

## Verification Path
Add a debug-only env gate (e.g. `BYRO_FSR_FORCE_DISPATCH_FAIL=1`) making the FFI shim's `dispatch` return `Err` without calling into the SDK, to isolate "recovery is sound when nothing was recorded." Separately, run `BYRO_VALIDATION=1` with a genuinely invalid dispatch description to see whether the SDK records before validating. Confirming signal: validation reporting an `oldLayout` mismatch on the depth or output image only on the forced-failure frame.

## Related
CHAIN-D2-02 (same boundary, filed separately), commit `f9a42e07` ("survive an FSR dispatch failure instead of dropping the frame").

## Suggested Fix
Not on reasoning alone. If the SDK is confirmed to record before it can fail, the robust shape is recording the FSR boundary barriers + dispatch into a secondary command buffer that is simply not executed on failure — a real restructure, not to be attempted without the repro above.

## Completeness Checks
- [ ] **DROP**: N/A pending validation result

---
