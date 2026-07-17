# PERF-D1-2026-07-16-01: M42 AI-package systems allocate a fresh per-frame decision Vec

**Labels**: low, performance, bug

**Severity**: LOW
**Dimension**: CPU Hot Paths
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`byroredux/src/systems/{wander,travel,follow,escort,guard,patrol}.rs` (one `Vec::new()` each) and `sandbox.rs:152,169,171` (two Vecs + a HashMap)

## Description
Each of the seven M42 AI-package runtimes allocates `let mut decisions: Vec<Decision> = Vec::new()` fresh every frame instead of the closure-captured persistent-scratch pattern `make_animation_system`/`make_billboard_system` already use. **Opt-in only** — all seven are registered behind per-behavior env-var gates in `boot.rs:721-754`, never in the default scheduler, so this costs nothing in normal play. No dhat coverage exists for this class of render/ECS-adjacent site.

Verified current: `wander.rs:207`, `travel.rs:121`, `follow.rs:90`, `escort.rs:126`, `guard.rs:105`, `patrol.rs:63` each still declare a fresh `Vec::new()` per invocation; `sandbox.rs:152,169,171` still allocates two Vecs and a HashMap per call.

## Suggested Fix
Convert each to a `make_*_system()` factory capturing persistent scratch reused via `clear()`. Low priority given opt-in gating.

## Completeness Checks
- [ ] **SIBLING**: Mirror the existing `make_animation_system`/`make_billboard_system` persistent-scratch pattern across all seven systems
- [ ] **TESTS**: A regression test pins this specific fix (e.g. asserting no per-frame allocation growth via a scratch-capacity check)
