# SUBSYS-01: Scale/shear baked into NiTransform.rotation is silently discarded at parse time

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2456
**Finding ID**: SUBSYS-01 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 7 — Subsystem coverage vs legacy
**Location**: `crates/nif/src/rotation.rs:11-64`; `crates/nif/src/import/coord.rs:33-63`; `crates/nif/src/stream.rs:679,702`
**Status**: NEW

## Description
A non-orthonormal 3×3 (exporter-baked uniform scale, non-uniform scale, or shear) is destroyed rather than decomposed. `sanitize_rotation`, for `|det−1| ≥ 0.1`, replaces the matrix with the nearest orthogonal one via SVD (`repair_rotation_svd_or_identity`), discarding the singular values instead of folding them into `NiTransform.scale`. Matrices inside the `det≈[0.9,1.1]` window but still non-orthonormal (e.g. `diag(2, 0.5, 1)`) take the fast path and are force-normalised by the #333 unit-quaternion guard. Nothing is logged either way.

## Evidence
`crates/nif/src/import/tests/transform.rs:250-270` pins the *loss* directly: a parent rotation of `2·I` composed with a child at (3,4,5) asserts a composed translation of **(3,4,5)**; Gamebryo's `NiTransform::operator*` computes `translate + scale·(rotate·child.translate)`, which for this input is **(6,8,10)** — the matrix IS applied in the source engine.

## Impact
Any subtree under an exporter-baked scaled node is placed at the wrong offset by exactly the discarded scale factor. Silent — presents downstream as "mesh part in the wrong place" with no breadcrumb, and affected content cannot be enumerated. Rare in Bethesda vanilla (dedicated scale float used instead) but reachable in 3rd-party/modded NIFs and older NetImmerse-era content.

## Suggested Fix
In `sanitize_rotation`, decompose (fold SVD singular values' geometric mean into the caller's `NiTransform.scale`) rather than orthogonalise-and-discard. Minimum viable step: emit a rate-limited `log::warn!` when a *scaled* (not zeroed) degenerate matrix is detected, to measure real corpus incidence before committing to the decomposition work.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the corrected composed translation for the `2·I` parent-rotation case from `transform.rs:250-270`
- [ ] **SIBLING**: Check all other `sanitize_rotation`/`is_degenerate_rotation` call sites for the same silent-discard behavior
