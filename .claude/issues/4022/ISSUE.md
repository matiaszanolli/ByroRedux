# #4022 — REN-2026-09-06-D20-02: #3570's D16 refusal has no channel back to its only consumer — `depth.stats` reports "armed, run again" forever on a device without `D32_SFLOAT`

**Labels**: low, renderer, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D20-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs` (`depth_capture_record_copy`), `crates/core/src/ecs/resources/mod.rs` (`DepthCaptureBridge`), `byroredux/src/commands/depth.rs` (`DepthStatsCommand`)
- **Status**: NEW
- **Description**: `26f9ddf4` correctly refuses to record a capture when
  `self.depth_format != vk::Format::D32_SFLOAT`, and does so *before* arming
  `depth_capture_pending_readback` — so the readback decode can never
  misinterpret a `D16_UNORM` buffer as `f32` samples. That half is sound.

  The refusal is reported only through `log::warn!`, and it happens *after*
  `self.depth_capture_requested.swap(false, Ordering::AcqRel)` has already
  consumed the request. `DepthCaptureBridge` carries exactly two channels —
  `requested: Arc<AtomicBool>` and `result: Arc<Mutex<Option<DepthCapture>>>` —
  with no way to express "refused". `DepthStatsCommand::execute` therefore takes
  the `None` result, re-arms, and returns *"depth capture armed — run
  `depth.stats` again in a frame or two to read it"*. On a device that selected
  `D16_UNORM`, every invocation forever returns that same line, with the only
  explanation buried in the renderer's log stream.
- **Evidence**:
  - `depth_capture_record_copy` swaps the request flag first, then
    `if self.depth_format != vk::Format::D32_SFLOAT { log::warn!(…); return; }`.
  - `find_depth_format` (`crates/renderer/src/vulkan/context/helpers.rs`)
    iterates `[vk::Format::D32_SFLOAT, vk::Format::D16_UNORM]` — the D16 arm is
    reachable, which is the whole premise of #3570 (Vulkan mandates D16 depth
    attachments; D32_SFLOAT is not guaranteed).
  - `DepthCaptureBridge` (`crates/core/src/ecs/resources/mod.rs`) has no error
    or refusal field, and `take_result()` cannot distinguish "not ready yet"
    from "will never be ready".
- **Impact**: Diagnostic UX only, and only on hardware without D32_SFLOAT depth
  attachments (not the dev RTX 4070 Ti). But #3308's step-2 comparison gate is
  exactly the sort of thing run on an unfamiliar machine, and the failure mode
  is an unbounded "come back later" with no visible cause — the same class of
  silent-dead-end #3570 was closing on the decode side.
- **Related**: #3570, #3308, `record_copy_refuses_non_d32_sfloat_before_arming_the_pending_readback`;
  `crates/renderer/src/vulkan/context/screenshot.rs` (the sibling bridge, whose
  owner tag gives it a place to report a claim outcome).
- **Suggested Fix**: Add a one-shot refusal channel to `DepthCaptureBridge`
  (e.g. `unsupported: Arc<AtomicBool>` set once by `depth_capture_record_copy`
  alongside the warn) and have `DepthStatsCommand` print the actual reason —
  "depth capture unsupported on this device (depth format is not D32_SFLOAT)" —
  instead of re-arming. Cheaper alternative: refuse *before* the
  `swap`, so the request stays pending and the state is at least inspectable.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix
