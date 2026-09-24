# #4798: PERF-D1-2026-09-23b-05: #4607's fix left one fresh per-frame Vec and the redundant `wanted` set in `GroundCoverResidency::reconcile`

**Severity**: LOW
**Labels**: low, performance, terrain-exterior, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D1-2026-09-23b-05)

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/render/groundcover.rs:118-203` (the return `.collect()` at `:190-201`; `wanted` at `:90,127-128`)
- **Status**: NEW (a residual of CLOSED #4607, not a regression)
- **Description**: A ~5 KB `Vec` is allocated every exterior frame. `wanted` duplicates `desired.contains_key`, adding ~135 inserts and ~135 lookups per frame.
- **Suggested Fix**: Return into a persistent field, and replace `wanted` with `desired.contains_key`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
