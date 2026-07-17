# SF-D8-01: .mat-arm comment claims metalness/roughness overrides become NaN in translate_material; they are always Some(classify_legacy_pbr) from import

**Severity**: LOW
**Labels**: low, documentation
**Location**: `byroredux/src/asset_provider/material.rs:596-605`
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (SF-D8-01)

## Description
The comment states overrides "stay `None` until Phase 2 walks the CDB" and become `f32::NAN`, filled by `resolve_pbr`'s NaN-sentinel classifier. This is factually wrong for every real Starfield mesh: import unconditionally sets `Some(legacy_pbr.metalness/roughness)` *before* the `.mat` arm runs, which returns early without touching those fields. The NaN-sentinel path never fires for Starfield content.

## Impact
Documentation only — the emitted `Material` is correct. Risk is to a future CDB Phase-2 implementer reasoning about the wrong mechanism.

## Suggested Fix
Correct the comment to state overrides are already `Some(classify_legacy_pbr(...))` from NIF import; Phase 2 must *overwrite* those `Some` values with CDB-authored ones rather than relying on an unreachable NaN-sentinel path.

## Completeness Checks
No rows apply — this is a documentation-only fix.
