# AUD-2026-09-21-D4-01: headless() migration stopped short — three tests still open the real audio device under comments that call them headless

**Issue**: #4740
**Filed**: 2026-09-22 (audit-publish, AUDIT_AUDIO_2026-09-22.md)

**Severity**: LOW
**Dimension**: 4 — Manager & ECS Lifecycle
**Location**: `byroredux/src/asset_provider/audio.rs:499`, `:515`, `:537` (comments `:492-495`, `:507-510`); `crates/audio/src/tests.rs:517` (`underwater_listener_state_persists`)

## Description
`2e2f40b23` added `AudioWorld::headless()` and moved `systems/audio.rs` tests to it, but three REGN-music tests in `asset_provider/audio.rs` and one crate test still call `AudioWorld::default()` (= `new()`, which opens a real cpal device + kira backend thread on a host with a free device). Two of the three carry comments claiming they're already "headless" — true only on a device-less host. Four hand-written inactive `AudioWorld { manager: None, .. }` literals in `crates/audio/src/tests.rs` duplicate what `headless()` already provides.

## Evidence
`git grep -n 'AudioWorld::default()' byroredux/src` → the three `asset_provider/audio.rs` lines. `lib.rs:440-444` (`Default` → `new()`), `:460` (`AudioManager::new`).

## Impact
Host-dependent unit tests: pass on both arms today, but open a real device on developer machines and run a different branch there than in CI — pinning less than they read as pinning.

## Related
`2e2f40b23` (the partial sweep).

## Suggested Fix
Switch the three `asset_provider/audio.rs` sites and `underwater_listener_state_persists` to `AudioWorld::headless()`; fix the two comments; replace the four inactive literals with `headless()`.
