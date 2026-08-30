# #3706 — ECS-2026-08-30-D10-06 (LATENT): sample_blended_transform has no single-layer short-circuit — 2x the per-bone lookup cost on the steady-state stack

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: Animation Runtime
**Location**: `crates/core/src/animation/stack.rs` (`sample_blended_transform`, ~:368-462; the duplicated triple at ~:379-388 and ~:422-432)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-06)

> **LATENT — no live repro.** `AnimationStack` is never registered in production (see ECS-D10-01/02/03 for the full reachability note). Do not hunt for an in-game repro.

## Description

The **allocation check passes** — the function allocates nothing; all state is scalar/`Vec3`/`Quat` locals. But it always runs both passes, so a stack with one layer (the common case once a fade completes) pays `registry.get()` twice, `effective_weight()` twice, and `clip.channels.get(&channel_name)` (a `HashMap` probe) twice per bone per frame. #288 reduced three passes to two; the degenerate one-layer case was never carved out.

## Evidence

The same `registry.get(layer.clip_handle)` / `layer.effective_weight()` / `clip.channels.get(&channel_name)` triple appears in the max-priority pass and again in the blend pass.

## Impact

~2x the intended per-bone lookup cost on the steady-state single-layer stack. Latent today.

## Suggested Fix

Early-out when `stack.layers.len() == 1`: resolve clip + channel once, apply the weight cull, and return the raw `sample_*` triple without the normalisation pass (a no-op at `w = ew/ew = 1.0`).

## Completeness Checks
- [ ] **SIBLING**: The float/color/bool channel blend siblings checked for the same duplicated probe
- [ ] **TESTS**: A regression test pins that the one-layer short-circuit produces bit-identical output to the two-pass path
