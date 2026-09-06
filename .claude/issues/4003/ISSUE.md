# #4003 — REN-2026-09-06-D10-03: on a non-`D32_SFLOAT` device `depth.stats` becomes an unbreakable "armed — come back in a frame or two" loop with no console-visible reason

**Labels**: low, memory, renderer, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D10-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Camera-Relative Precision
- **Location**: `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`), `crates/renderer/src/vulkan/context/depth_capture.rs` (`VulkanContext::depth_capture_record_copy`)
- **Status**: NEW (the console-facing consequence of the just-landed #3570; the refusal
  itself is correct — see Coverage)
- **Description**: `#3570` correctly made `depth_capture_record_copy` refuse rather than
  misdecode when `find_depth_format` selected `D16_UNORM`, and it consumes the request flag
  before returning, so the refusal is a clean no-op with no layout or leak consequence.
  What it has no path back to is the caller. `DepthCaptureBridge::take_result()` will
  return `None` forever, so `depth.stats` takes its "nothing landed yet" branch on every
  invocation: it re-arms and prints "depth capture armed — run `depth.stats` again in a
  frame or two to read it". The only signal that the capture will *never* land is a
  `log::warn!` in the renderer's log stream, which a `byro-dbg` console session does not
  see.
- **Evidence**: `execute` has exactly two exits for a present bridge — the `take_result()`
  `None` arm (arm + "come back") and the report arm — and no third arm for "capture
  refused". The refusal in `depth_capture_record_copy` writes nothing to
  `depth_capture_result` and never arms `depth_capture_pending_readback`.
- **Impact**: `depth.stats` is the *live* half of #3308's before/after comparison gate
  (the analytic half being `Camera::depth_resolution_at` / `_reversed`). On any device
  whose `find_depth_format` falls through to `D16_UNORM` — Vulkan mandates `D16_UNORM`
  depth-attachment support but not `D32_SFLOAT`, which is the whole premise of #3570 — the
  gate silently has no live half, and the operator's evidence is an instruction to keep
  waiting. Zero impact on the dev RTX 4070 Ti. Note this is *distinct* from the already-filed
  #3571 (`REN-2026-08-30-D10-02`), which is about the gate being runnable only
  pre-conversion; this is about the gate being unrunnable at all on some hardware, with no
  way to tell.
- **Related**: #3570 (`26f9ddf4`), #3308, #3571 (distinct — do not merge), #3630 (the
  degenerate-camera rejection line, which is the precedent for surfacing a refusal to the
  console instead of leaving it ambiguous).
- **Suggested Fix**: Give the refusal a return channel — e.g. have `depth_capture_record_copy`
  store a `DepthCapture`-shaped `Err`/unsupported marker on the bridge (or expose the
  selected `depth_format` on the bridge at init), and add a third arm to
  `DepthStatsCommand::execute` printing "depth capture unsupported: device selected
  {format}, not D32_SFLOAT (#3570)". Mirrors the shape #3630 already established for the
  degenerate-camera case, and stops the command from arming a request that can never
  complete.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix
