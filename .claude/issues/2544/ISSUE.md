# SAFE-2026-08-07-02: fsr3-sys's Vulkan smoke example -- 20 of 23 unsafe blocks/fns carry no SAFETY comment

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2544
**Finding ID**: SAFE-2026-08-07-02

**Severity**: MEDIUM
**Dimension**: 4 (Unsafe-Block Discipline)
**Location**: `crates/fsr3-sys/examples/vulkan_context_smoke.rs:52,63,64,86,101,107,110,112,117,120,124,134-135,139,177,198,200,215,221` (also the two `unsafe fn` declarations at `:50` `run()` and `:116` `create_and_destroy_context()`, neither carrying a `# Safety` doc block)
**Status**: NEW (pre-existing since the file's introduction on 2026-07-22, `34e26ca8`; not touched since 2026-08-03 — a scope gap in the prior audit, not a regression)

## Description
The one example binary in the workspace doing raw `ash` FFI (`cargo run -p byroredux-fsr3-sys --example vulkan_context_smoke`) sits outside every `src/` tree the 2026-08-03 audit explicitly scanned. It has 23 `unsafe` occurrences and only 3 `SAFETY` comments (`validation_callback`'s `CStr::from_ptr` at `:24`, a blanket comment on `main()`'s call into `run()` at `:38-39`, and one on `ash::Entry::load()` at `:51`). Everything else — instance/device creation, debug-messenger create/destroy, physical-device/queue-family enumeration, extension enumeration, `CStr::from_ptr` for extension-name compare, `get_physical_device_features2`, `Context::create`, `device_wait_idle`, `destroy_device` — has no individual justification. A "does this unsafe block have a SAFETY comment" convention check silently skips this file if scoped to `src/` as the prior audit was.

## Evidence
Confirmed directly: `grep -c unsafe crates/fsr3-sys/examples/vulkan_context_smoke.rs` → 23, `grep -c SAFETY` → 3. No other `examples/` file in the workspace contains any `unsafe` at all — isolated gap, not a systemic `examples/` problem.

## Impact
Low blast radius — opt-in smoke-test binary, not linked into the engine or any `cargo test` run. Manual inspection shows the sequence is actually correct (device → context → `device_wait_idle` → context dropped → device destroyed → debug messenger destroyed → instance destroyed, proper reverse order, `?`-propagated errors throughout). The gap is discipline/documentation: a future editor extending this file has no per-call precondition text to check their change against.

## Suggested Fix
Add per-call `// SAFETY:` comments matching house style, or at minimum widen `run()`'s existing blanket comment at `:38-39` to explicitly cover every raw call inside it and note that `create_and_destroy_context` inherits the same contract.

## Completeness Checks
- [ ] **UNSAFE**: Each of the 23 `unsafe` sites gets a `// SAFETY:` comment or is covered by a widened blanket comment stating the upheld invariant
