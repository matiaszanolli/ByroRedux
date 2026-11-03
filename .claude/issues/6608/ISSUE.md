# RT-2026-10-31-01: Skyrim exterior to interior door transition loses the Vulkan device (NVRM Xid 109 CTX SWITCH TIMEOUT)

Labels: high,renderer,vulkan,sync,bug,game:skyrim
Source: docs/audits/AUDIT_RUNTIME_2026-10-31.md

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-31.md` RT-2026-10-31-01 (report HEAD `670f81917`).
**Re-verified at HEAD `7f2d98944`**: Not re-run at publish (the gate is display-only). No commit since the report touches `byroredux/src/cell_loader/transition.rs`; renderer work since (M-LIGHT Phases 2-3, the fire local grid) has not been through this gate.

- **Severity**: HIGH (the engine exits, with the playable path's second Bannered Mare entry as the trigger)
- **Dimension**: Gate matrix / renderer runtime
- **Location**: gate `docs/smoke-tests/p1-character-traversal.sh` (skyrim_se); path `byroredux/src/cell_loader/transition.rs` → first draw of the destination cell (`app_frame` "Draw failed")
- **Status**: NEW
- **Game / Cell**: skyrim_se / Whiterun exterior `(6,-2)` to `WhiterunBanneredMare` (the route's second door; the first visit, interior to exterior, is fine and is what `p0` covers)
- **Baseline**: none (the 10-27 audit ran P1 headless and reported it NOT RUN)
- **Current**: 3 of 4 display runs end with the same lines. The 4th run, with `BYRO_VALIDATION=1`, timed out earlier at "door prompt recovered after walking", so it never reached the door.
  ```
  loading.screen: begin LSCR=0001DD5D owner=Door
  Transition: source REFR 00000000 → interior 'WhiterunBanneredMare' …
  (5 s later) ERROR app_frame: Draw failed: wait_for_fences: The logical device has been lost
  ```
  Kernel log: `NVRM: Xid (PCI:0000:01:00): 109 … name=byroredux, channel 0x00000039, errorString CTX SWITCH TIMEOUT` at 17:26:55, 18:24:22 and 18:41:45 (this audit's three runs), with an identical `Info` word on two of them. The same Xid 109 fired three times in 90 s at 00:31 to 00:33 the same day, so it predates this audit. `Xid 56` display-engine errors also appear.
- **Description**: The second transition logs `Transition: source REFR … → interior …` but never reaches `Transition: unloading prior exterior` or `Cell transition applied`. The main thread stalls about 5 s, then the first `wait_for_fences` returns `DEVICE_LOST`. A GPU job that runs past the driver's context-switch timeout is the proximate cause. Which pass it is, and whether it is driver-side or a malformed dispatch, is not established. I did not bisect, since each release build takes minutes and the gate is display-only. Skyrim has no other exterior to interior round trip: `p0` is one-way, `p5-door-transition` is not declared for Skyrim, and `m-exteriors` has no door leg. FNV's `p5-door-transition` (both directions) passes.
- **Evidence**: `/tmp/audit/runtime/gates/p1-skyrim_se.log`, `r-p1-skyrim_se-b.log`, `/tmp/audit/runtime/p1_c.log` (engine.stderr kept by `BYROREDUX_KEEP_ARTIFACTS=1`), `journalctl -k | grep Xid`. `nvidia-smi`: RTX 4070 Ti, driver 580.178.04.
- **Impact**: The engine dies on a normal Skyrim door transition; save-guard, streaming and loading-screen work all sit behind it. It cannot be found headless.
- **Related**: #6506 (the shutdown leak ERROR that follows any device loss is the same teardown path). `/audit-renderer` and `/audit-concurrency` (queue/AS sync) should look at the exterior-unload to interior-load GPU work (BLAS/TLAS eviction and rebuild, SVGF history reset).
- **Suggested Fix**: Capture one run under `BYRO_VALIDATION=1` with the slower pacing kept out of the route (drive the second door through a `byro-dbg` command instead of frame-counted input), then bisect with `p1` on a display. Add a Skyrim `p5-door-transition` route so the round trip has a headless-capable gate.

## Completeness Checks
- [ ] **SIBLING**: The other exterior <-> interior door routes (FNV, FO3, FO4) checked under the same pacing
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: The gate is re-run live and passes (or SKIPs with 77 on missing data)
