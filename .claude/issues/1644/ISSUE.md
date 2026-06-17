# SAFE-2026-06-16-01: ~327 renderer unsafe blocks lack SAFETY comments (outside #1432-fixed files)

**Issue**: #1644
**Severity**: MEDIUM
**Dimension**: 4 — Unsafe-Block Discipline
**Labels**: medium, safety, renderer, documentation
**Source**: docs/audits/AUDIT_SAFETY_2026-06-16.md
**Location**: `crates/renderer/src/vulkan/`

## Description
Renderer carries 616 `unsafe` tokens / ~235 `SAFETY` mentions. Of 544 non-test
`unsafe {` blocks, ~327 have no SAFETY comment within the preceding 8 lines.
Some are false positives (one per-function comment covering batched ash `cmd_*`
calls) but a large genuine residue remains. Per the unified-severity Special
Rules table, `unsafe` without a safety comment = MEDIUM; reported as one batched
finding.

## Evidence (verified at publish)
- `acceleration/blas_static.rs` — 35 `unsafe {` / 8 SAFETY
- `composite.rs` — 41 / 15
- `volumetrics.rs` — 27 / 4
- `bloom.rs` — 21 uncommented
- `buffer.rs` — 25 / 10
- `context/draw.rs` — ~20 uncommented
- long tail: `context/helpers.rs` 16, `texture.rs` 15, `context/mod.rs` 15,
  `skin_compute.rs` 13, `device.rs` 13, `context/resize.rs` 12,
  `texture_registry.rs` 10
- Excluded (CLOSED #1432, commit ec23ed1a): `gpu_timers.rs` (29/29, confirmed
  lockstep), `blas_skinned.rs`.

## Impact
Documentation / maintainability debt, not a live soundness bug. Sampled blocks
hold no FALSE invariant — ash create/destroy/dispatch FFI wrappers sound by
surrounding handle lifetime. Risk: an uncommented block masks a future invariant
break during refactor.

## Related
CLOSED #1432 (SAFE-U6), CLOSED #1403/#1408/#1415/#1416/#1425.

## Suggested Fix
Continue #1432's tiered policy — trivial destroy/create get a one-line note,
sync-dependent calls a full comment. Prioritize blas_static.rs, composite.rs,
volumetrics.rs, bloom.rs, buffer.rs, draw.rs.
