# Issue #456

FO3-6-03: ROADMAP 'Megaton 42 FPS / 199 textures' claim predates TAA/SVGF/BLAS batching

---

## Severity: Medium (doc + bench hygiene)

**Location**: `ROADMAP.md:30-31`, `docs/engine/game-compatibility.md:60-65`

## Problem

"Megaton Player House: 1609 entities, 199 textures, 42 FPS" claim repeated verbatim in two docs. No checked-in bench log, no CI target, no `--bench-frames` coverage for Megaton.

The 42 FPS figure predates:
- M37.5 TAA (Halton jitter + YCoCg variance clamp)
- M37 SVGF indirect lighting denoiser
- M31 BLAS batched builds + LRU eviction
- M31.5 streaming RIS (8 reservoirs/fragment)

Likely off by 2× today in either direction.

## Impact

Stale docs mislead compatibility/performance claims. Can't detect regressions without a baseline.

## Fix

1. Add Megaton cell to the CI bench suite alongside sweetroll (Prospector FPS).
2. Check in a bench snapshot with current commit hash.
3. Either confirm the 42 FPS figure or edit both docs to match the new measurement.

## Completeness Checks

- [ ] **TESTS**: `--bench-frames` CI entry for Megaton Player House
- [ ] **SIBLING**: Audit other hardcoded FPS claims in ROADMAP.md / docs/engine/ — any others that predate M31+?
- [ ] **DOCS**: Append commit hash or date to all performance claims (`42 FPS @ <hash>`)

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-6-03)
