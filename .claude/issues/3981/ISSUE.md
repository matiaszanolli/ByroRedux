# #3981 — REN-2026-09-06-D12-02: `taa_failed` / `svgf_failed` can never latch — `c43cb269`'s #3605 fix sits on an unreachable branch, and the one reachable TAA failure is warn-only

**Labels**: medium, pipeline, renderer, tech-debt, test-gap, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D12-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/taa.rs` (`TaaPipeline::dispatch`), `crates/renderer/src/vulkan/svgf.rs` (`SvgfPipeline::dispatch`), `crates/renderer/src/vulkan/context/post_passes.rs` (`record_taa_pass`, `record_svgf_pass`), `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (the `taa.upload_params` call site)
- **Status**: NEW
- **Description**: `TaaPipeline::dispatch` and `SvgfPipeline::dispatch` are
  declared `-> Result<()>` but their bodies contain **no error-producing
  construct at all** — no `?`, no `return Err`, no `bail!`, no `.context(`,
  no `map_err` — and both end in an unconditional `Ok(())`. Every statement
  is an infallible `ash` command recording call. Therefore
  `if let Err(e) = taa.dispatch(…)` in `record_taa_pass` and the matching arm
  in `record_svgf_pass` are dead code, and `self.taa_failed` /
  `self.svgf_failed` can never become `true` at runtime (they are only ever
  set to `false`, at construction and on resize).
  Three consequences:
  1. `c43cb269` ("Fix #3605: signal a temporal discontinuity on TAA dispatch
     failure", 2026-09-05) adds `signal_temporal_discontinuity(
     TAA_DISPATCH_FAILURE_RECOVERY_FRAMES)` inside that dead arm. Its
     regression guard,
     `record_taa_pass_signals_temporal_discontinuity_on_dispatch_failure`,
     is a source-scan that asserts the *text* is present — so it passes while
     the behaviour is unreachable. The hazard #3605 describes is real; the
     fix as landed cannot fire.
  2. The `#1932` un-jitter gate
     (`assemble_camera_and_lights` gates Halton jitter on
     `taa.is_some() && !taa_failed`) and the `fall_back_to_raw_hdr` reroute
     are likewise unreachable.
  3. The TAA failure that **is** reachable is a different one:
     `taa.upload_params` (which does `param_buffers[frame].write_mapped(…)`
     — a mapped-slice/flush operation, the same fallible class #2504
     hardened for `upload_indirect_draws`) is handled with a bare
     `log::warn!("TAA upload_params failed: {e}")` and no latch. On that
     path the dispatch still runs, against whatever the params UBO held
     before (a previous frame's, or uninitialised on a slot's first use),
     while the geometry pass has already rendered jittered. That is exactly
     the "jittered but unresolved" state #3605 exists to protect against,
     and it is the case with no protection.
- **Evidence**: extracting each `dispatch` body by brace matching and
  grepping for `?;`, `return Err`, `bail!`, `Err(`, `.context(`, `map_err`
  yields zero hits for `taa.rs`, `svgf.rs` (and `bloom.rs`, whose arm is a
  harmless `warn!` with no latch); `ssao.rs`, `volumetrics.rs` and
  `caustic.rs` by contrast do contain `?`, so the fallible-dispatch shape is
  genuine elsewhere in the same file's call sequence — this is a
  three-pipeline anomaly, not a blanket convention.
  `crates/renderer/src/vulkan/context/mod.rs` documents `taa_failed` as
  "first `taa.dispatch` error in a session".
- **Impact**: A documented per-pass permanent-failure recovery tier (named as
  such in `record_post_passes`'s own doc: "the per-pass permanent-failure
  latches are preserved exactly") does not exist for SVGF or TAA. No runtime
  misbehaviour today — nothing fails, so nothing is mishandled — but two
  shipped fixes (#1932, #3605) and one reroute are unverifiable dead weight,
  and the real failure mode (a params-upload failure) silently degrades to a
  stale-parameter TAA resolve on a jittered frame. Also a maintenance trap:
  the source-scan guard gives false confidence that the path is exercised.
- **Related**: #3605 / REN-2026-08-30-D13-02 (`c43cb269`), #1932 / TAA-D13-01
  (the jitter gate), #917 / REN-D10-NEW-03 (`dispatched_this_frame`), #2504 /
  D12-2026-08-07-02 (the same fallible-upload class, correctly handled for
  indirect draws), #2146 / #917 (why `record_post_passes` is infallible).
- **Suggested Fix**: Route the reachable failure into the existing latch
  instead of inventing a new one: make the `taa.upload_params` /
  `svgf.upload_params` call sites set `taa_failed` / `svgf_failed` on `Err`
  (they already run in `build_and_upload_instances`, before
  `record_post_passes`, so the `!self.taa_failed` gate at the dispatch site
  picks it up in the same frame and the jitter gate picks it up the next).
  That makes #3605's `signal_temporal_discontinuity` live — but see
  **D12-03**, which must be fixed in the same change. Separately, either drop
  the `-> Result<()>` on the two dispatches or add a comment saying it is
  reserved; a `Result` no producer can populate is what made the dead arm
  look alive to three successive audits.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
