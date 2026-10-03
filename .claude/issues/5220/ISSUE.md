# #5220 — REN-D12-2026-10-03-01: `DebugStats::groundcover_model_{demanded,emitted}` (#4920) are written every frame but nothing reads them

**Labels**: low,renderer,terrain-exterior,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Debug/Telemetry (co-owner `/audit-exterior`)
- **Location**: `crates/core/src/ecs/resources/mod.rs` (`DebugStats::groundcover_model_demanded` / `groundcover_model_emitted`); writer in `byroredux/src/app_events.rs` (`ctx.groundcover_model_stats().unwrap_or_default()`); producer `GroundCoverModelTier::harvest` / `stats()` in `crates/renderer/src/vulkan/groundcover_models.rs`.
- **Status**: NEW. This is a residual of CLOSED #4920. Its suggested fix was "Log once when a cap is hit, and add both counts to `DebugStats`". Both were done, but no consumer was ever wired.
- **Description**: 4dfe97f3e added the two fields, and the commit message says the truncation is "reported as DebugStats::groundcover_model_{demanded, emitted}". The `app_events.rs` comment says it is "visible without `--bench-*`". A whole-tree grep finds the fields only at their definition, their `Default`, and that one writer. No surface reads them:
  - The `stats` console command (`commands/world_info.rs`) does not.
  - The debug server's `eval_stats` (`DebugResponse::Stats`) does not.
  - `log_stats_system` does not.
  - No debug-ui panel does.

  The visibility that does exist comes from the once-per-episode `log::warn!` in `harvest` and the bench-only `groundcover-models:` line.

  The value is also stale off-cover. `harvest` returns early unless `pending_stats[frame]` is set, and `record` sets it only on frames that dispatch. In an interior, or with no cover, `stats` keeps the last exterior placement indefinitely. The field doc says "read back one pipelined frame late", which does not describe that latch.
- **Evidence**: `grep -rn 'groundcover_model_demanded\|groundcover_model_emitted' crates byroredux tools` returns `resources/mod.rs` (definition and Default) and `app_events.rs` (write) only. `GroundCoverModelTier::prepare` calls `self.harvest(device, frame)`, and `harvest` begins with `if !std::mem::take(&mut self.pending_stats[frame]) { return; }`.
- **Impact**: The fields are dead telemetry. Someone triaging missing plants through `byro-dbg` has no way to see the truncation counts #4920 meant to expose, short of a bench run or catching the one-time warn. If a reader is added later, it will show a stale exterior count in interiors. No render impact.
- **Related**: #4920 (CLOSED); #4338 (the blade-tier precedent: log only).
- **Suggested Fix**: Either surface the pair (one `stats` line, or a `DebugResponse::Stats` field), zero or invalidate `stats` on a frame where `prepare` finds nothing to place, and fix the field doc; or delete the fields and leave the warn and bench line as the documented surfaces.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
