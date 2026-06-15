# AUD-2026-06-14-04: SoundCache docstring references stale resolve_footstep_sound; live fn is try_load_default_footstep

- **Issue**: #1615
- **Severity**: LOW
- **Labels**: low, tech-debt, documentation
- **Dimension**: Gameplay Audio Wiring (docstring integrity)
- **Location**: `crates/audio/src/lib.rs:1157` (docstring); live fn at `byroredux/src/asset_provider.rs:410`
- **Source report**: `docs/audits/AUDIT_AUDIO_2026-06-14.md`

## Description
The `SoundCache` docstring (#859 note) names `byroredux/src/asset_provider.rs::resolve_footstep_sound`. No such function exists; the live function is `try_load_default_footstep` (`asset_provider.rs:410`), which does exactly what the docstring describes (writes the decoded `Arc` into `FootstepConfig.default_sound`, bypassing the cache).

## Evidence
- `grep resolve_footstep_sound` matches only the docstring at `lib.rs:1157` (re-confirmed 2026-06-15); actual loader `try_load_default_footstep` at `asset_provider.rs:410`, called from `main.rs:554`.

## Impact
Doc-accuracy only; described behavior is correct, only the symbol name is stale.

## Related
#1614 (AUD-...-03); skill Phase-1 step 5 pre-flagged this rot.

## Suggested Fix
One-word edit — `resolve_footstep_sound` → `try_load_default_footstep` at `lib.rs:1157`.
